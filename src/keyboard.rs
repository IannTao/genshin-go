//! Keyboard Tab reading-order focus and post-dialog focus restoration.
use nsh_core::{
    element::{Input, OrderWatch, create_root, watch_order},
    keymap::When,
    prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const FOCUS_RING: AdaptiveColor = AdaptiveColor::rgb(0x0013_6658, 0x007e_e0c5);

/// Identifies a registered keyboard-focusable control so focus can be restored to it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusHandle {
    id: u64,
}

#[derive(Clone)]
struct FocusScope(Rc<dyn Fn() -> bool>);

enum StopKind {
    Button(Rc<dyn Fn()>),
    Input,
}

struct FocusStop {
    id: u64,
    element: ElementRef,
    visible: Rc<dyn Fn() -> bool>,
    kind: StopKind,
}

#[derive(Clone, Copy)]
struct FocusTarget {
    id: u64,
    element: ElementRef,
    is_input: bool,
}

impl FocusStop {
    fn target(&self) -> FocusTarget {
        FocusTarget {
            id: self.id,
            element: self.element,
            is_input: matches!(self.kind, StopKind::Input),
        }
    }
}

struct FocusInner {
    stops: Rc<RefCell<Vec<FocusStop>>>,
    next_id: Cell<u64>,
    focused: Signal<Option<u64>>,
    keyboard_visible: Signal<bool>,
    order: OrderWatch,
}

/// Manages keyboard reading-order focus within one launcher instance.
#[derive(Clone)]
pub(crate) struct KeyboardFocus(Rc<FocusInner>);

impl KeyboardFocus {
    /// Installs reading-order Tab, Shift+Tab, Enter, and Space bindings in the current context.
    pub(crate) fn install() -> Self {
        let stops = Rc::new(RefCell::new(Vec::<FocusStop>::new()));
        let focused = signal(None::<u64>);
        let keyboard_visible = signal(false);
        let order = {
            let listed = stops.clone();
            let sorted = stops.clone();
            watch_order(
                move || listed.borrow().iter().map(|stop| stop.element).collect(),
                move |indices| {
                    let mut stops = sorted.borrow_mut();
                    let mut taken: Vec<Option<FocusStop>> = stops.drain(..).map(Some).collect();
                    for &index in indices {
                        if let Some(stop) = taken.get_mut(index).and_then(Option::take) {
                            stops.push(stop);
                        }
                    }
                    stops.extend(taken.into_iter().flatten());
                },
            )
        };
        let manager = Self(Rc::new(FocusInner {
            stops,
            next_id: Cell::new(1),
            focused,
            keyboard_visible,
            order,
        }));
        provide_context(manager.clone());
        let forward = manager.clone();
        on_key_with(
            "tab",
            KeyOptions {
                when: When::Always,
                repeat: true,
            },
            move |_| forward.step(false),
        );
        let backward = manager.clone();
        on_key_with(
            "shift+tab",
            KeyOptions {
                when: When::Always,
                repeat: true,
            },
            move |_| backward.step(true),
        );
        let enter = manager.clone();
        on_key_with(
            "enter",
            KeyOptions {
                when: When::NotEditing,
                repeat: false,
            },
            move |_| enter.activate(),
        );
        let space = manager.clone();
        on_key_with(
            "space",
            KeyOptions {
                when: When::NotEditing,
                repeat: false,
            },
            move |_| space.activate(),
        );
        manager
    }

    fn register(
        &self,
        element: ElementRef,
        extra_visible: Option<Rc<dyn Fn() -> bool>>,
        kind: StopKind,
    ) -> FocusHandle {
        let scope = use_context::<FocusScope>();
        let visible: Rc<dyn Fn() -> bool> = match (scope, extra_visible) {
            (Some(scope), Some(extra)) => Rc::new(move || (scope.0)() && extra()),
            (Some(scope), None) => scope.0,
            (None, Some(extra)) => extra,
            (None, None) => Rc::new(|| true),
        };
        let id = self.0.next_id.get();
        self.0.next_id.set(id + 1);
        self.0.stops.borrow_mut().push(FocusStop {
            id,
            element,
            visible,
            kind,
        });
        self.0.order.invalidate();
        let inner = self.0.clone();
        cleanup(move || {
            inner.stops.borrow_mut().retain(|stop| stop.id != id);
            if inner.focused.try_get() == Some(Some(id)) {
                inner.focused.try_set(None);
            }
            inner.order.invalidate();
        });
        FocusHandle { id }
    }

    fn is_ring_shown(&self, handle: FocusHandle) -> bool {
        self.0.keyboard_visible.get() && self.0.focused.get() == Some(handle.id)
    }

    fn previous_input(&self, next_id: u64) -> Option<ElementRef> {
        let current = self.0.focused.get_untracked()?;
        (current != next_id).then(|| {
            self.0
                .stops
                .borrow()
                .iter()
                .find(|stop| stop.id == current && matches!(stop.kind, StopKind::Input))
                .map(|stop| stop.element)
        })?
    }

    fn apply_focus(&self, target: FocusTarget, show_ring: bool, move_platform_focus: bool) {
        let prev_input = self.previous_input(target.id);
        self.0.focused.set(Some(target.id));
        self.0.keyboard_visible.set(show_ring);
        if let Some(prev) = prev_input {
            prev.blur();
        }
        if move_platform_focus {
            if target.is_input {
                target.element.focus();
            } else {
                target.element.set_accessibility_focus();
            }
        }
    }

    fn mark_pointer_focus(&self, handle: FocusHandle) {
        let target = self
            .0
            .stops
            .borrow()
            .iter()
            .find(|stop| stop.id == handle.id)
            .map(FocusStop::target);
        if let Some(target) = target {
            self.apply_focus(target, false, false);
        }
    }

    fn mark_input_focused(&self, handle: FocusHandle) {
        self.0.focused.set(Some(handle.id));
    }

    fn mark_input_blurred(&self, handle: FocusHandle) {
        if self.0.focused.get_untracked() == Some(handle.id) {
            self.0.focused.set(None);
        }
    }

    fn step(&self, backward: bool) {
        let current = self.0.focused.get_untracked();
        let target = {
            let stops = self.0.stops.borrow();
            let active: Vec<FocusTarget> = stops
                .iter()
                .filter(|stop| (stop.visible)())
                .map(FocusStop::target)
                .collect();
            if active.is_empty() {
                return;
            }
            let current_pos = current.and_then(|id| active.iter().position(|item| item.id == id));
            let next_pos = match (current_pos, backward) {
                (Some(pos), false) => (pos + 1) % active.len(),
                (Some(pos), true) => (pos + active.len() - 1) % active.len(),
                (None, false) => 0,
                (None, true) => active.len() - 1,
            };
            active[next_pos]
        };
        self.apply_focus(target, true, true);
    }

    fn activate(&self) {
        let current = self.0.focused.get_untracked();
        let action = current.and_then(|id| {
            self.0.stops.borrow().iter().find_map(|stop| {
                if stop.id == id
                    && (stop.visible)()
                    && let StopKind::Button(action) = &stop.kind
                {
                    Some(action.clone())
                } else {
                    None
                }
            })
        });
        if let Some(action) = action {
            self.0.keyboard_visible.set(true);
            action();
        }
    }

    fn focus_handle(&self, handle: FocusHandle) {
        let target = self
            .0
            .stops
            .borrow()
            .iter()
            .find(|stop| stop.id == handle.id && (stop.visible)())
            .map(FocusStop::target);
        if let Some(target) = target {
            self.apply_focus(target, true, true);
        }
    }
}

/// Evaluates `render` under a child visibility scope so hidden panels are skipped by Tab.
pub(crate) fn with_scope<R>(visible: impl Fn() -> bool + 'static, render: impl FnOnce() -> R) -> R {
    let parent = use_context::<FocusScope>();
    let combined: Rc<dyn Fn() -> bool> = match parent {
        Some(parent) => Rc::new(move || (parent.0)() && visible()),
        None => Rc::new(visible),
    };
    let (owner, result) = create_root(move || {
        provide_context(FocusScope(combined));
        render()
    });
    cleanup(move || owner.dispose());
    result
}

/// Registers a button view whose action receives its own focus handle for post-dialog restoration.
pub(crate) fn register_button_with_handle(
    button: View,
    action: impl Fn(Option<FocusHandle>) + 'static,
    enabled: Option<Rc<dyn Fn() -> bool>>,
) -> (View, Option<FocusHandle>) {
    let slot = Rc::new(Cell::new(None));
    let callback_slot = slot.clone();
    let wrapped: Rc<dyn Fn()> = Rc::new(move || action(callback_slot.get()));
    let (view, handle) = register_button(button, wrapped, enabled);
    slot.set(handle);
    (view, handle)
}

/// Registers a button view in the active reading-order focus chain and returns its focus handle.
pub(crate) fn register_button(
    button: View,
    action: Rc<dyn Fn()>,
    enabled: Option<Rc<dyn Fn() -> bool>>,
) -> (View, Option<FocusHandle>) {
    let Some(focus) = use_context::<KeyboardFocus>() else {
        return (button.on_press(move || action()), None);
    };
    let element = element_ref();
    let handle = focus.register(element, enabled, StopKind::Button(action.clone()));
    let ring_focus = focus.clone();
    let press_focus = focus;
    let view = button
        .element_ref(&element)
        .on_press(move || {
            press_focus.mark_pointer_focus(handle);
            action();
        })
        .style_when(
            move || ring_focus.is_ring_shown(handle),
            |style| {
                style
                    .ring(px(2.0))
                    .ring_color(FOCUS_RING)
                    .ring_offset(px(1.0))
            },
        );
    (view, Some(handle))
}

/// Registers a text input in the active reading-order focus chain.
pub(crate) fn register_input(field: Input) -> Input {
    let Some(focus) = use_context::<KeyboardFocus>() else {
        return field;
    };
    let element = element_ref();
    let handle = focus.register(element, None, StopKind::Input);
    let on_focus = focus.clone();
    let on_blur = focus;
    field
        .element_ref(&element)
        .on_focus(move || on_focus.mark_input_focused(handle))
        .on_blur(move || on_blur.mark_input_blurred(handle))
}

/// Restores keyboard focus and accessibility focus to a previously registered control.
pub(crate) fn restore_focus(handle: Option<FocusHandle>) {
    if let (Some(focus), Some(handle)) = (use_context::<KeyboardFocus>(), handle) {
        focus.focus_handle(handle);
    }
}

/// Captures the current keyboard focus context for restoration inside an async task.
pub(crate) fn current_focus() -> Option<KeyboardFocus> {
    use_context::<KeyboardFocus>()
}

/// Restores keyboard focus using a previously captured focus context.
pub(crate) fn restore_focus_with(focus: Option<&KeyboardFocus>, handle: Option<FocusHandle>) {
    if let (Some(focus), Some(handle)) = (focus, handle) {
        focus.focus_handle(handle);
    }
}
