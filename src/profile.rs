//! Locally managed game UID spaces and the current verified account.
use crate::launcher::{LauncherEnvironment, button};
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
    roster: Option<impl IntoElement>,
) -> impl IntoElement {
    let message = signal(String::new());
    let managing = signal(false);
    let growth_revision = signal(0_u64);
    let switch = environment.clone();
    #[cfg(target_os = "windows")]
    let local_records = environment.uid_database_path().map(|path| {
        view()
            .gap_2()
            .child(crate::inventory::section(
                path.clone(),
                profile,
                history_revision,
            ))
            .child(crate::growth::section(
                path.clone(),
                profile,
                growth_revision,
                history_revision,
            ))
            .child(crate::backup::section(
                path.clone(),
                profile,
                history_revision,
                growth_revision,
            ))
            .child(crate::wishes::section(path, profile, history_revision))
    });
    #[cfg(not(target_os = "windows"))]
    let local_records = None::<View>;
    view()
        .gap_2()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    text("角色资料")
                        .text_size(px(24.0))
                        .font_weight(FontWeight::BOLD),
                )
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
        .child(local_records)
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
        .child(
            nsh_core::element::input()
                .test_id("game-uid")
                .label("游戏 UID")
                .h_10()
                .px_3()
                .bg(move || theme().secondary)
                .placeholder("输入要添加的游戏 UID")
                .value(move || input.get())
                .on_change(move |value: &str| input.set(value.to_owned())),
        )
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

fn history_controls(
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
    managing: Signal<bool>,
    message: Signal<String>,
    revision: Signal<u64>,
    growth_revision: Signal<u64>,
) -> impl IntoElement {
    let owner = Owner::current();
    dynamic(move || {
        profile.get().active_uid.map(|uid| {
            let environment = environment.clone();
            view()
                .gap_2()
                .child(button("资料管理", move || {
                    managing.update(|open| *open = !*open);
                }))
                .child(managing.get().then(|| {
                    button("删除历史记录", move || {
                        let uid = uid.clone();
                        let environment = environment.clone();
                        message.set(String::new());
                        owner.run(|| {
                            confirm_history_deletion(
                                environment,
                                uid,
                                profile,
                                managing,
                                message,
                                revision,
                                growth_revision,
                            );
                        });
                    })
                }))
        })
    })
}

fn confirm_history_deletion(
    environment: Rc<dyn LauncherEnvironment>,
    uid: String,
    profile: Signal<Profile>,
    managing: Signal<bool>,
    message: Signal<String>,
    revision: Signal<u64>,
    growth_revision: Signal<u64>,
) {
    let answer = alert_dialog()
        .title(format!("删除 UID {uid} 的历史记录？"))
        .description(
            "将清除该 UID 在本机数据库中保存的各类历史。UIGF、UIAF 和背包的备份及导出文件可通过文件管理器单独管理。该 UID 的数据空间、其他 UID 的历史和米游社授权会保留。",
        )
        // The first button receives the Windows default focus when there is no regular action.
        .cancel("cancel", "取消")
        .destructive("delete", format!("确认删除 UID：{uid}"))
        .show();
    spawn(async move {
        if !answer
            .await
            .is_ok_and(|result| result.action.as_deref() == Some("delete"))
        {
            return;
        }
        match environment.delete_uid_history(&uid) {
            Ok(state) => {
                profile.set(state);
                revision.update(|revision| *revision += 1);
                growth_revision.update(|revision| *revision += 1);
                managing.set(false);
                message.set(format!("已清除 UID {uid} 的本机历史记录"));
            }
            Err(error) => message.set(format!("删除历史记录失败：{error}")),
        }
    });
}
