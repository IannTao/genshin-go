use super::{
    ACCENT, INK, NAV_HOVER_BG, SURFACE, WorkbenchAppearance, icon_svg, icons, styled_tooltip,
};
use nsh_core::Checked;
use nsh_core::element::{ImageSrc, svg};
use nsh_core::prelude::*;
use std::rc::Rc;

fn topbar_icon_button(
    test_id: &'static str,
    label: impl Fn() -> &'static str + Copy + 'static,
    state: impl Fn() -> AccessibilityState + 'static,
    icon: impl Fn() -> &'static str + 'static,
    action: impl Fn() + 'static,
) -> View {
    crate::keyboard::register_button(
        view()
            .test_id(test_id)
            .w(px(34.0))
            .h(px(34.0))
            .rounded(px(6.0))
            .items_center()
            .justify_center()
            .role(AccessibilityRole::Button)
            .label(label)
            .accessibility_state(state)
            .hover(|style| style.bg(NAV_HOVER_BG))
            .tooltip(styled_tooltip(Placement::Bottom, 6.0, false, label))
            .child(
                svg(move || ImageSrc::svg_markup(icon()))
                    .w(px(20.0))
                    .h(px(20.0)),
            ),
        Rc::new(action),
        None,
    )
    .0
}

pub(super) fn topbar(appearance: WorkbenchAppearance) -> View {
    let sidebar_toggle = topbar_icon_button(
        "launcher-sidebar-toggle",
        move || {
            if appearance.is_collapsed() {
                "展开侧栏"
            } else {
                "收起侧栏"
            }
        },
        move || AccessibilityState {
            expanded: Some(!appearance.is_collapsed()),
            ..Default::default()
        },
        move || {
            if appearance.is_collapsed() {
                icons::PANEL_LEFT_OPEN
            } else {
                icons::PANEL_LEFT_CLOSE
            }
        },
        move || appearance.toggle_sidebar(),
    );
    let theme_toggle = topbar_icon_button(
        "launcher-theme-toggle",
        move || {
            if WorkbenchAppearance::is_dark() {
                "切换浅色模式"
            } else {
                "切换深色模式"
            }
        },
        move || AccessibilityState {
            pressed: Some(if WorkbenchAppearance::is_dark() {
                Checked::True
            } else {
                Checked::False
            }),
            ..Default::default()
        },
        move || {
            if WorkbenchAppearance::is_dark() {
                icons::SUN
            } else {
                icons::MOON
            }
        },
        WorkbenchAppearance::toggle_dark,
    );
    view()
        .test_id("launcher-topbar")
        .h(px(46.0))
        .flex_shrink(0.0)
        .flex_row()
        .items_center()
        .px_3()
        .gap_3()
        .bg(SURFACE)
        .text_color(INK)
        .child(sidebar_toggle)
        .child(
            view()
                .test_id("launcher-brand")
                .flex_row()
                .items_center()
                .gap_2()
                .child(icon_svg(icons::COMPASS, 20.0).text_color(ACCENT))
                .child(
                    text("Genshin Go")
                        .text_size(px(15.0))
                        .font_weight(FontWeight::BOLD),
                ),
        )
        .child(view().flex_1())
        .child(theme_toggle)
}
