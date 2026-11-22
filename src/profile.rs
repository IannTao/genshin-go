//! Locally managed game UID spaces and the current verified account.
use crate::keyboard::{
    FocusHandle, current_focus, register_input, restore_focus, restore_focus_with,
};
use crate::launcher::{LauncherEnvironment, button, focusable_button};
use nsh_base::{alert_dialog, theme};
use nsh_core::prelude::*;
use std::rc::Rc;

/// Identifies the saved `MiHoYo` account and its verified game UID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedAccount {
    /// Identifies the `MiHoYo` account.
    pub account_id: String,
    /// Identifies the game character verified for that account.
    pub uid: String,
    /// Identifies the saved authorization used for concurrent changes.
    pub revision: String,
}

/// Describes the locally available UID spaces and current authorization.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Profile {
    /// Contains the UIDs of the local data spaces.
    pub uids: Vec<String>,
    /// Contains the UID selected for browsing local data.
    pub active_uid: Option<String>,
    /// Contains the sole locally saved account, when authorized.
    pub account: Option<VerifiedAccount>,
}

/// Checks the syntax of a game UID before using it as a local key.
pub(crate) fn validate_uid(uid: &str) -> Result<(), String> {
    if matches!(uid.len(), 9 | 10)
        && !uid.starts_with('0')
        && uid.bytes().all(|byte| byte.is_ascii_digit())
    {
        Ok(())
    } else {
        Err("请输入 9 或 10 位有效游戏 UID".into())
    }
}

/// Renders the local UID data spaces backed by the launcher's shared profile.
pub(crate) fn spaces(
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
    history_revision: Signal<u64>,
    growth_revision: Signal<u64>,
    load_error: Option<String>,
    roster: Option<impl IntoElement>,
    growth: Option<View>,
) -> impl IntoElement {
    let message = signal(String::new());
    let managing = signal(false);
    let switch = environment.clone();
    view()
        .gap_2()
        .child(load_error.map(text))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(text(move || {
                    profile.get().active_uid.map_or_else(
                        || "尚未选择游戏 UID".into(),
                        |uid| format!("当前游戏 UID：{uid}"),
                    )
                }))
                .child(dynamic(move || {
                    let state = profile.get();
                    state
                        .uids
                        .into_iter()
                        .filter(|uid| Some(uid) != state.active_uid.as_ref())
                        .map(|uid| {
                            let environment = switch.clone();
                            button(format!("切换至 UID：{uid}"), move || match environment
                                .select_uid(&uid, false)
                            {
                                Ok(state) => {
                                    profile.set(state);
                                    managing.set(false);
                                    message.set(String::new());
                                }
                                Err(error) => message.set(error),
                            })
                            .into_element()
                        })
                        .collect::<Vec<_>>()
                })),
        )
        .child(uid_creation(
            environment.clone(),
            profile,
            managing,
            message,
        ))
        .child(history_controls(
            environment,
            profile,
            managing,
            message,
            history_revision,
            growth_revision,
        ))
        .child(text(move || message.get()))
        .child(roster)
        .child(growth)
}

fn uid_creation(
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
    managing: Signal<bool>,
    message: Signal<String>,
) -> impl IntoElement {
    let input = signal(String::new());
    view()
        .gap_2()
        .child(register_input(
            nsh_core::element::input()
                .test_id("game-uid")
                .label("游戏 UID")
                .h_10()
                .px_3()
                .bg(move || theme().secondary)
                .placeholder("输入要添加的游戏 UID")
                .value(move || input.get())
                .on_change(move |value: &str| input.set(value.to_owned())),
        ))
        .child(button("创建 UID 数据空间", move || {
            let value = input.get_untracked();
            let uid = value.trim();
            if uid.is_empty()
                && let Some(account) = profile.get_untracked().account
            {
                message.set(format!(
                    "扫码验证的 UID {} 已有数据空间。添加其他角色时，请输入游戏 UID。",
                    account.uid
                ));
                return;
            }
            match validate_uid(uid).and_then(|()| environment.select_uid(uid, true)) {
                Ok(state) => {
                    profile.set(state);
                    managing.set(false);
                    input.set(String::new());
                    message.set(format!("已打开 UID {uid} 的数据空间"));
                }
                Err(error) => message.set(error),
            }
        }))
}

#[derive(Clone, Copy)]
struct HistoryControls {
    owner: Owner,
    profile: Signal<Profile>,
    managing: Signal<bool>,
    message: Signal<String>,
    revision: Signal<u64>,
    growth_revision: Signal<u64>,
    manage_handle: Signal<Option<FocusHandle>>,
    focus_manage_button: Signal<bool>,
}

fn history_controls(
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
    managing: Signal<bool>,
    message: Signal<String>,
    revision: Signal<u64>,
    growth_revision: Signal<u64>,
) -> impl IntoElement {
    let controls = HistoryControls {
        owner: Owner::current(),
        profile,
        managing,
        message,
        revision,
        growth_revision,
        manage_handle: signal(None),
        focus_manage_button: signal(false),
    };
    dynamic(move || {
        controls.profile.get().active_uid.map(|uid| {
            let environment = environment.clone();
            let (manage_button, handle) = focusable_button("资料管理", move |_| {
                controls.managing.update(|open| *open = !*open);
            });
            controls.manage_handle.set(handle);
            if controls.focus_manage_button.get_untracked() {
                controls.focus_manage_button.set(false);
                restore_focus(handle);
            }
            let delete_uid = uid.clone();
            let delete_env = environment;
            view().gap_2().child(manage_button).child(when(
                move || controls.managing.get(),
                move || {
                    let uid = delete_uid.clone();
                    let environment = delete_env.clone();
                    let (delete_button, _) =
                        focusable_button("删除历史记录", move |trigger| {
                            let uid = uid.clone();
                            let environment = environment.clone();
                            controls.message.set(String::new());
                            controls.owner.run(|| {
                                controls.confirm_deletion(environment, uid, trigger);
                            });
                        });
                    delete_button
                },
            ))
        })
    })
}

impl HistoryControls {
    fn confirm_deletion(
        self,
        environment: Rc<dyn LauncherEnvironment>,
        uid: String,
        trigger: Option<FocusHandle>,
    ) {
        let focus = current_focus();
        let answer = alert_dialog()
            .title(format!("删除 UID {uid} 的历史记录？"))
            .description(format!(
                "将清除 UID {uid} 在本机数据库中的祈愿、成就、已拥有角色、背包、养成计划与手填库存。备份与导出文件可通过文件管理器单独管理。该 UID 的数据空间、其他 UID 的记录和米游社授权会保留。"
            ))
            // The first button receives the Windows default focus when there is no regular action.
            .cancel("cancel", "取消")
            .destructive("delete", format!("确认删除 UID：{uid}"))
            .show();
        spawn(async move {
            if !answer
                .await
                .is_ok_and(|result| result.action.as_deref() == Some("delete"))
            {
                restore_focus_with(focus.as_ref(), trigger);
                return;
            }
            match environment.delete_uid_history(&uid) {
                Ok(state) => {
                    let profile_changed = self.profile.get_untracked() != state;
                    self.managing.set(false);
                    if profile_changed {
                        self.focus_manage_button.set(true);
                    }
                    self.profile.set(state);
                    self.revision.update(|revision| *revision += 1);
                    self.growth_revision.update(|revision| *revision += 1);
                    self.message.set(format!("已清除 UID {uid} 的本机历史记录"));
                    if !profile_changed {
                        restore_focus_with(focus.as_ref(), self.manage_handle.get_untracked());
                    }
                }
                Err(error) => {
                    self.message.set(format!("删除历史记录失败：{error}"));
                    restore_focus_with(focus.as_ref(), trigger);
                }
            }
        });
    }
}
