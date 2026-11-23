use super::{
    ACCENT, BORDER, INK, MUTED_INK, NAV_ACTIVE_BG, NAV_ACTIVE_INK, NAV_HOVER_BG, Page, SURFACE,
    WorkbenchAppearance, icon_svg, styled_tooltip,
};
use nsh_core::AccessibilityCurrent;
use nsh_core::prelude::*;
use std::rc::Rc;

pub(super) fn sidebar(
    page: Signal<Page>,
    appearance: WorkbenchAppearance,
    download_status: View,
) -> View {
    let settings = view()
        .test_id("launcher-sidebar-bottom")
        .gap_2()
        .child(view().h(px(1.0)).bg(BORDER))
        .child(page_button(page, Page::Settings, appearance));
    view()
        .test_id("launcher-sidebar")
        .w(move || px(appearance.sidebar_width()))
        .flex_shrink(0.0)
        .p(px(10.0))
        .gap_1()
        .bg(SURFACE)
        .text_color(INK)
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view().gap_1().children(
                    Page::MAIN
                        .iter()
                        .map(|&target| page_button(page, target, appearance).into_element()),
                ),
            ),
        )
        .child(download_status.hover(|style| style.bg(NAV_HOVER_BG)))
        .child(settings)
}

fn page_button(page: Signal<Page>, target: Page, appearance: WorkbenchAppearance) -> View {
    let selected = move || page.get() == target;
    let meta = target.meta();
    let button = view()
        .test_id(meta.test_id)
        .h(px(38.0))
        .rounded(px(7.0))
        .flex_row()
        .items_center()
        .px(move || px(if appearance.is_collapsed() { 4.0 } else { 8.0 }))
        .gap(move || px(if appearance.is_collapsed() { 0.0 } else { 8.0 }))
        .role(AccessibilityRole::Button)
        .label(meta.label)
        .accessibility_state(move || AccessibilityState {
            selected: selected(),
            current: selected().then_some(AccessibilityCurrent::Page),
            ..Default::default()
        })
        .bg(move || if selected() { NAV_ACTIVE_BG } else { SURFACE })
        .hover(|style| style.bg(NAV_HOVER_BG))
        .tooltip(styled_tooltip(
            Placement::Right,
            8.0,
            move || !appearance.is_collapsed(),
            meta.label,
        ))
        .child(view().w(px(3.0)).h(px(20.0)).rounded(px(2.0)).bg(move || {
            if selected() {
                ACCENT
            } else {
                AdaptiveColor::TRANSPARENT
            }
        }))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .flex_row()
                .items_center()
                .gap_3()
                .style_when(move || appearance.is_collapsed(), Styled::justify_center)
                .child(icon_svg(meta.icon, 20.0).text_color(move || {
                    if selected() {
                        NAV_ACTIVE_INK
                    } else {
                        MUTED_INK
                    }
                }))
                .child(
                    text(meta.label)
                        .text_size(px(14.0))
                        .text_color(move || if selected() { NAV_ACTIVE_INK } else { INK })
                        .style_when(selected, |style| style.font_weight(FontWeight::BOLD))
                        .style_when(move || appearance.is_collapsed(), Styled::hidden),
                ),
        );
    crate::keyboard::register_button(button, Rc::new(move || page.set(target)), None).0
}

pub(super) fn panel<E: IntoElement>(
    page: Signal<Page>,
    target: Page,
    appearance: WorkbenchAppearance,
    content: impl FnOnce() -> E,
) -> Scroll {
    let body = crate::keyboard::with_scope(move || page.get_untracked() == target, content);
    scroll()
        .flex_1()
        .style_when(move || page.get() != target, Styled::hidden)
        .child(
            view()
                .w_full()
                .min_w(px(0.0))
                .p(move || {
                    px(if appearance.narrow_content() {
                        12.0
                    } else {
                        28.0
                    })
                })
                .gap_3()
                .child(
                    view().test_id("launcher-page-title").child(
                        text(target.label())
                            .text_size(px(24.0))
                            .font_weight(FontWeight::BOLD),
                    ),
                )
                .child(body.into_element()),
        )
}
