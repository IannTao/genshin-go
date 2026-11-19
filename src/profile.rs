//! Locally managed game UID spaces and the current verified account.
use crate::launcher::{LauncherEnvironment, button};
use nsh_base::theme;
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

/// Renders account and UID controls backed by the same local profile.
pub(crate) fn profile_controls(environment: Rc<dyn LauncherEnvironment>) -> impl IntoElement {
    let (initial, error) = match environment.load_profile() {
        Ok(profile) => (profile, None),
        Err(error) => (
            Profile::default(),
            Some(format!("读取本机数据失败：{error}")),
        ),
    };
    let profile = signal(initial);
    view()
        .gap_4()
        .child(crate::accounts::accounts(environment.clone(), profile))
        .child(spaces(environment, profile, error))
}

fn spaces(
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
    load_error: Option<String>,
) -> impl IntoElement {
    let input = signal(String::new());
    let message = signal(load_error.unwrap_or_default());
    let create = environment.clone();
    let switch = environment.clone();
    view()
        .gap_2()
        .child(text("UID 数据空间"))
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
                            message.set(String::new());
                        }
                        Err(error) => message.set(error),
                    })
                    .into_element()
                })
                .collect::<Vec<_>>()
        }))
        .child(
            nsh_core::element::input()
                .test_id("game-uid")
                .label("游戏 UID")
                .h_12()
                .px_3()
                .bg(move || theme().secondary)
                .placeholder("游戏 UID")
                .value(move || input.get())
                .on_change(move |value: &str| input.set(value.to_owned())),
        )
        .child(button("创建 UID 数据空间", move || {
            let value = input.get_untracked();
            let uid = value.trim();
            match validate_uid(uid).and_then(|()| create.select_uid(uid, true)) {
                Ok(state) => {
                    profile.set(state);
                    input.set(String::new());
                    message.set(String::new());
                }
                Err(error) => message.set(error),
            }
        }))
        .child(text(move || message.get()))
        .child(history_controls(environment, profile))
}

#[derive(Clone, PartialEq)]
enum HistoryDeletion {
    Closed,
    Choosing,
    Confirming(String),
}

fn history_controls(
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
) -> impl IntoElement {
    let deletion = signal(HistoryDeletion::Closed);
    let message = signal(String::new());
    view()
        .gap_2()
        .child(dynamic(move || {
            let uids = profile.get().uids;
            match deletion.get() {
                HistoryDeletion::Closed if uids.is_empty() => None,
                HistoryDeletion::Closed => Some(
                    button("删除历史记录", move || {
                        message.set(String::new());
                        deletion.set(HistoryDeletion::Choosing);
                    })
                    .into_element(),
                ),
                HistoryDeletion::Choosing => Some(
                    view()
                        .gap_2()
                        .child(text("选择要删除历史记录的 UID"))
                        .child(
                            uids.into_iter()
                                .map(|uid| {
                                    button(format!("选择 UID：{uid}"), move || {
                                        message.set(String::new());
                                        deletion.set(HistoryDeletion::Confirming(uid.clone()));
                                    })
                                    .into_element()
                                })
                                .collect::<Vec<_>>(),
                        )
                        .child(button("关闭历史记录删除", move || {
                            deletion.set(HistoryDeletion::Closed);
                        }))
                        .into_element(),
                ),
                HistoryDeletion::Confirming(uid) => {
                    let environment = environment.clone();
                    Some(
                        view()
                            .gap_2()
                            .child(text(format!("本次删除涉及的 UID：{uid}")))
                            .child(text(
                                "此操作将清除该 UID 的本机历史记录，保留数据空间和授权。",
                            ))
                            .child(button(format!("确认删除 UID：{uid}"), move || {
                                match environment.delete_uid_history(&uid) {
                                    Ok(state) => {
                                        profile.set(state);
                                        deletion.set(HistoryDeletion::Closed);
                                        message.set(format!("已清除 UID {uid} 的本机历史记录"));
                                    }
                                    Err(error) => message.set(format!("删除历史记录失败：{error}")),
                                }
                            }))
                            .child(button("取消删除历史记录", move || {
                                message.set(String::new());
                                deletion.set(HistoryDeletion::Choosing);
                            }))
                            .into_element(),
                    )
                }
            }
        }))
        .child(text(move || message.get()))
}
