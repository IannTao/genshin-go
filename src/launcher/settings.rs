use super::{ACCENT, CARD, CONTROL, INK, MUTED_INK, WorkbenchAppearance, action_button};
use nsh_core::prelude::*;

pub(super) type BackupSectionBuilder = Box<dyn FnOnce() -> View>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Appearance,
    #[cfg(target_os = "windows")]
    Backup,
}

fn settings_option_button(
    test_id: &'static str,
    label: &'static str,
    selected: impl Fn() -> bool + Copy + 'static,
    action: impl Fn() + 'static,
) -> View {
    action_button(label, action)
        .test_id(test_id)
        .accessibility_state(move || AccessibilityState {
            selected: selected(),
            ..Default::default()
        })
        .bg(move || if selected() { ACCENT } else { CONTROL })
        .text_color(move || if selected() { white().into() } else { INK })
        .style_when(selected, |style| style.font_weight(FontWeight::BOLD))
}

fn settings_card(title: &'static str, description: &'static str, controls: View) -> View {
    view()
        .p_4()
        .rounded(px(10.0))
        .bg(CARD)
        .gap_2()
        .child(
            text(title)
                .text_size(px(16.0))
                .font_weight(FontWeight::BOLD),
        )
        .child(text(description).text_size(px(13.0)).text_color(MUTED_INK))
        .child(controls)
}

fn appearance_settings(appearance: WorkbenchAppearance) -> View {
    view()
        .test_id("settings-appearance")
        .gap_3()
        .child(settings_card(
            "界面主题",
            "选择浅色或深色。顶栏与此处反映同一主题状态。",
            view()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(settings_option_button(
                    "theme-option-light",
                    "浅色",
                    move || !WorkbenchAppearance::is_dark(),
                    move || WorkbenchAppearance::set_dark(false),
                ))
                .child(settings_option_button(
                    "theme-option-dark",
                    "深色",
                    WorkbenchAppearance::is_dark,
                    move || WorkbenchAppearance::set_dark(true),
                )),
        ))
        .child(settings_card(
            "左侧导航",
            "收起后仅显示图标，悬停可查看模块名称。",
            view()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(settings_option_button(
                    "sidebar-option-expanded",
                    "展开",
                    move || !appearance.is_collapsed(),
                    move || appearance.set_sidebar_collapsed(false),
                ))
                .child(settings_option_button(
                    "sidebar-option-collapsed",
                    "仅图标",
                    move || appearance.is_collapsed(),
                    move || appearance.set_sidebar_collapsed(true),
                )),
        ))
}

pub(super) fn settings_page(
    appearance: WorkbenchAppearance,
    backup_section: Option<BackupSectionBuilder>,
) -> View {
    let tab = signal(SettingsTab::Appearance);
    let mut tabs = view()
        .test_id("settings-tabs")
        .flex_row()
        .flex_wrap()
        .gap_2()
        .child(settings_option_button(
            "settings-tab-appearance",
            "外观",
            move || tab.get() == SettingsTab::Appearance,
            move || tab.set(SettingsTab::Appearance),
        ));
    #[cfg(target_os = "windows")]
    {
        tabs = tabs.child(settings_option_button(
            "settings-tab-backup",
            "备份与恢复",
            move || tab.get() == SettingsTab::Backup,
            move || tab.set(SettingsTab::Backup),
        ));
    }
    let appearance_panel = crate::keyboard::with_scope(
        move || tab.get_untracked() == SettingsTab::Appearance,
        || {
            appearance_settings(appearance)
                .style_when(move || tab.get() != SettingsTab::Appearance, Styled::hidden)
        },
    );
    let mut root = view().gap_4().child(tabs).child(appearance_panel);
    #[cfg(target_os = "windows")]
    if let Some(backup) = backup_section {
        let backup_panel = crate::keyboard::with_scope(
            move || tab.get_untracked() == SettingsTab::Backup,
            move || {
                view()
                    .test_id("settings-backup")
                    .style_when(move || tab.get() != SettingsTab::Backup, Styled::hidden)
                    .child(backup())
            },
        );
        root = root.child(backup_panel);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = backup_section;
    root
}
