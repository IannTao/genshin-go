//! Account authorization UI; external effects use the application's environment.
use crate::keyboard::{FocusHandle, current_focus, restore_focus_with};
use crate::launcher::{LauncherEnvironment, button, focusable_button};
use crate::profile::{Profile, VerifiedAccount};
use nsh_base::alert_dialog;
use nsh_core::prelude::*;
use std::{future::Future, pin::Pin, rc::Rc};

/// Represents an asynchronous account operation at the application boundary.
pub type AccountOperation<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;

/// Holds an active passport QR login session.
#[derive(Clone, PartialEq)]
pub struct QrSession {
    /// Contains the URL encoded into the QR image.
    pub url: String,
    /// Identifies the QR login attempt to the passport service.
    pub ticket: String,
    /// Identifies the device used for both creation and polling.
    pub device: String,
}

// Never derive Debug: this value contains an authorization secret.
/// Contains a `MiHoYo` account identifier and its in-memory session cookies.
#[derive(Clone)]
pub struct Authorization {
    /// Identifies a `MiHoYo` account rather than a game character.
    pub account_id: String,
    /// Contains the Cookie request header and must never be logged.
    pub cookie_header: String,
    /// Contains the optional session Cookie used for community sign-in submissions.
    pub community_cookie: Option<String>,
}

impl Authorization {
    /// Validates the account identifier and bounded Cookie payload.
    pub fn validate(&self) -> Result<(), String> {
        if self.account_id.is_empty()
            || self.account_id.len() > 20
            || !self.account_id.bytes().all(|c| c.is_ascii_digit())
            || self.cookie_header.is_empty()
            || self.cookie_header.len() > 8192
            || self
                .community_cookie
                .as_ref()
                .is_some_and(|cookie| cookie.is_empty() || cookie.len() > 8192)
        {
            return Err("账号授权数据无效".into());
        }
        Ok(())
    }
}

/// Describes the passport QR state visible to the launcher.
pub enum QrStatus {
    /// Indicates that the QR code has not been scanned.
    Waiting,
    /// Indicates that a phone has scanned the QR code.
    Scanned,
    /// Contains a confirmed account authorization.
    Confirmed(Authorization),
    /// Indicates that the QR code is no longer usable.
    Expired,
    /// Indicates that the scan was cancelled.
    Cancelled,
}

#[derive(Clone, PartialEq)]
enum Scan {
    Idle,
    Creating,
    Ready(String),
    Verifying,
    Retry,
}

fn qr_image(url: &str) -> Result<String, String> {
    use std::fmt::Write;
    let qr = qrcodegen::QrCode::encode_text(url, qrcodegen::QrCodeEcc::Medium)
        .map_err(|_| "二维码内容过长，请刷新二维码")?;
    let size = qr.size() + 8;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {size} {size}\"><rect width=\"{size}\" height=\"{size}\" fill=\"white\"/><path fill=\"black\" d=\""
    );
    for y in 0..qr.size() {
        for x in 0..qr.size() {
            if qr.get_module(x, y) {
                let _ = write!(svg, "M{},{}h1v1h-1z ", x + 4, y + 4);
            }
        }
    }
    svg.push_str("\"/></svg>");
    Ok(svg)
}

fn qr_element(url: &str) -> impl IntoElement + use<> {
    match qr_image(url) {
        Ok(markup) => nsh_core::element::svg(nsh_core::element::ImageSrc::svg_markup(&markup))
            .w(px(260.0))
            .h(px(260.0))
            .test_id("authorization-qr")
            .label("米游社授权二维码")
            .into_element(),
        Err(error) => text(error).into_element(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthorizationEntry {
    Account,
    SignIn,
}

#[derive(Clone, Copy)]
struct AccountControls {
    owner: Owner,
    profile: Signal<Profile>,
    message: Signal<String>,
    scan: Signal<Scan>,
    target: Signal<Option<VerifiedAccount>>,
    entry: Signal<AuthorizationEntry>,
    generation: Signal<u64>,
    checking: Signal<bool>,
    managing: Signal<bool>,
    focus_add_button: Signal<bool>,
}

/// Renders daily community sign-in and account authorization on the community page.
pub(crate) fn community_page(
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
) -> impl IntoElement {
    let controls = AccountControls {
        owner: Owner::current(),
        profile,
        message: signal(String::new()),
        scan: signal(Scan::Idle),
        target: signal(None),
        entry: signal(AuthorizationEntry::Account),
        generation: signal(0),
        checking: signal(false),
        managing: signal(false),
        focus_add_button: signal(false),
    };
    let scan = controls.scan;
    let entry = controls.entry;
    let managing = controls.managing;
    let focus_add_button = controls.focus_add_button;
    let sign_in_environment = environment.clone();
    let (today, history, needs_renewal) =
        crate::community_sign_in::controls(environment.clone(), profile, move |id| {
            let environment = sign_in_environment.clone();
            button(format!("重新扫码：{id}"), move || {
                if scan.get_untracked() == Scan::Idle {
                    entry.set(AuthorizationEntry::SignIn);
                    controls.start(environment.clone());
                }
            })
            .into_element()
        });
    let refresh_environment = environment.clone();
    let feedback = move || controls.feedback(refresh_environment.clone());
    view()
        .gap_2()
        .child(
            text(move || {
                profile.with(|profile| {
                    profile.account.as_ref().map_or_else(
                        || "米哈游账号".into(),
                        |account| format!("米哈游账号：{}", account.account_id),
                    )
                })
            })
            .text_size(px(24.0))
            .font_weight(FontWeight::BOLD),
        )
        .child(today)
        .child(when(
            move || entry.get() == AuthorizationEntry::SignIn,
            feedback.clone(),
        ))
        .child(history)
        .child(dynamic(move || {
            let idle = scan.get() == Scan::Idle;
            let start = environment.clone();
            let Some(account) = profile.get().account else {
                return view()
                    .gap_2()
                    .child(text("尚未保存米哈游账号"))
                    .child(idle.then(|| {
                        let (add_button, handle) =
                            focusable_button("扫码添加米哈游账号", move |_| {
                                entry.set(AuthorizationEntry::Account);
                                controls.start(start.clone());
                            });
                        if focus_add_button.get_untracked() {
                            focus_add_button.set(false);
                            crate::keyboard::restore_focus(handle);
                        }
                        add_button
                    }));
            };
            let id = account.account_id.clone();
            let environment = environment.clone();
            view()
                .gap_2()
                .child(text(format!("已验证游戏 UID：{}", account.uid)))
                .child((idle && !needs_renewal.get()).then(|| {
                    button(format!("重新扫码：{id}"), move || {
                        entry.set(AuthorizationEntry::Account);
                        controls.start(start.clone());
                    })
                }))
                .child(idle.then(|| {
                    button("管理授权", move || {
                        managing.update(|open| *open = !*open);
                    })
                }))
                .child(when(
                    move || idle && managing.get(),
                    move || controls.management(environment.clone(), account.clone()),
                ))
        }))
        .child(when(
            move || entry.get() == AuthorizationEntry::Account,
            feedback,
        ))
}

impl AccountControls {
    fn feedback(self, environment: Rc<dyn LauncherEnvironment>) -> impl IntoElement {
        let scan = self.scan;
        view()
            .gap_2()
            .child(dynamic(move || {
                (scan.get() != Scan::Idle)
                    .then(|| self.target.get())
                    .flatten()
                    .map(|account| text(format!("正在续期米哈游账号：{}", account.account_id)))
            }))
            .child(text(move || self.message.get()))
            .child(dynamic(move || match scan.get() {
                Scan::Ready(url) => Some(qr_element(&url)),
                Scan::Idle | Scan::Creating | Scan::Verifying | Scan::Retry => None,
            }))
            .child(when(
                move || scan.get() == Scan::Retry,
                move || {
                    let environment = environment.clone();
                    button("刷新二维码", move || self.start(environment.clone()))
                },
            ))
            .child(when(
                move || scan.get() != Scan::Idle,
                move || button("取消扫码", move || self.cancel_scan()),
            ))
    }

    fn start(self, environment: Rc<dyn LauncherEnvironment>) {
        if self.scan.get_untracked() == Scan::Idle {
            self.target.set(self.profile.get_untracked().account);
        }
        self.generation.update(|g| *g += 1);
        let current = self.generation.get_untracked();
        self.scan.set(Scan::Creating);
        self.message.set("正在获取二维码…".into());
        // Scan and management buttons can unmount while their operations are still running.
        self.owner.run(|| {
            spawn(self.authorize(environment, current, self.target.get_untracked()));
        });
    }

    fn cancel_scan(self) {
        self.generation.update(|g| *g += 1);
        self.scan.set(Scan::Idle);
        self.message.set("已取消扫码".into());
    }

    fn management(
        self,
        environment: Rc<dyn LauncherEnvironment>,
        account: VerifiedAccount,
    ) -> impl IntoElement {
        let check = environment.clone();
        let id = account.account_id.clone();
        let (remove_button, _) = focusable_button(
            format!("移除账号：{}", account.account_id),
            move |trigger| {
                self.confirm_removal(environment.clone(), account.clone(), trigger);
            },
        );
        view()
            .gap_2()
            .child(button(format!("检查授权：{id}"), move || {
                self.check(check.clone(), id.clone());
            }))
            .child(remove_button)
    }

    fn check(self, environment: Rc<dyn LauncherEnvironment>, id: String) {
        if self.checking.get_untracked() {
            return;
        }
        self.entry.set(AuthorizationEntry::Account);
        self.checking.set(true);
        let current = self.generation.get_untracked();
        self.message.set(format!("正在检查账号 {id} 授权…"));
        self.owner.run(|| {
            spawn(async move {
                let result = environment.check_authorization(id.clone()).await;
                self.checking.set(false);
                if self.generation.get_untracked() == current {
                    self.message.set(match result {
                        Ok(true) => format!("账号 {id} 米游社授权有效"),
                        Ok(false) => format!("账号 {id} 授权已失效，请重新扫码"),
                        Err(error) => error,
                    });
                }
            });
        });
    }

    fn confirm_removal(
        self,
        environment: Rc<dyn LauncherEnvironment>,
        account: VerifiedAccount,
        trigger: Option<FocusHandle>,
    ) {
        let focus = current_focus();
        self.owner.run(|| {
            let answer = alert_dialog()
                .title(format!("移除米哈游账号 {}？", account.account_id))
                .description(format!(
                    "移除后，米哈游账号 {} 的自动签到将暂停，需重新授权。各 UID 数据空间和本地角色资料会保留。",
                    account.account_id
                ))
                // Windows defaults to the first button when there is no regular action.
                .cancel("cancel", "取消")
                .destructive("remove", format!("确认移除账号：{}", account.account_id))
                .show();
            spawn(async move {
                if !answer
                    .await
                    .is_ok_and(|result| result.action.as_deref() == Some("remove"))
                {
                    restore_focus_with(focus.as_ref(), trigger);
                    return;
                }
                self.entry.set(AuthorizationEntry::Account);
                match environment.remove_authorization(&account) {
                    Ok(state) => {
                        self.generation.update(|g| *g += 1);
                        self.focus_add_button.set(true);
                        self.managing.set(false);
                        self.profile.set(state);
                        self.message.set("已移除米游社授权，已保留 UID 数据".into());
                    }
                    Err(error) => {
                        self.message.set(format!("移除账号失败：{error}"));
                        restore_focus_with(focus.as_ref(), trigger);
                    }
                }
            });
        });
    }

    async fn authorize(
        self,
        environment: Rc<dyn LauncherEnvironment>,
        current: u64,
        expected_account: Option<VerifiedAccount>,
    ) {
        let Self {
            generation,
            scan,
            message,
            profile,
            ..
        } = self;
        let result = environment.create_qr().await;
        if generation.get_untracked() != current {
            return;
        }
        match result {
            Ok(qr) => {
                scan.set(Scan::Ready(qr.url.clone()));
                message.set("等待扫描：请使用米游社扫码".into());
                loop {
                    nsh_core::element::timer(std::time::Duration::from_secs(2)).await;
                    if generation.get_untracked() != current {
                        return;
                    }
                    let result = environment.poll_qr(qr.clone()).await;
                    if generation.get_untracked() != current {
                        return;
                    }
                    match result {
                        Ok(QrStatus::Waiting) => continue,
                        Ok(QrStatus::Scanned) => {
                            message.set("等待确认：请在米游社确认授权".into());
                            continue;
                        }
                        Ok(QrStatus::Confirmed(authorization)) => {
                            scan.set(Scan::Verifying);
                            message.set("正在验证游戏 UID 归属…".into());
                            let result = verify_authorization(
                                environment.as_ref(),
                                &authorization,
                                expected_account.as_ref(),
                            )
                            .await;
                            if generation.get_untracked() != current {
                                return;
                            }
                            scan.set(Scan::Idle);
                            let uid = match result {
                                Ok(uid) => uid,
                                Err(error) => {
                                    message.set(error);
                                    break;
                                }
                            };
                            match environment.save_authorization(
                                authorization,
                                &uid,
                                expected_account.as_ref(),
                            ) {
                                Ok(state) => {
                                    profile.set(state);
                                    let status = if expected_account.is_some() {
                                        "米游社授权已续期"
                                    } else {
                                        "米游社授权已保存"
                                    };
                                    message.set(status.into());
                                }
                                Err(error) => {
                                    let mut status = format!("保存米游社授权失败：{error}");
                                    match environment.load_profile() {
                                        Ok(state) => profile.set(state),
                                        Err(_) => status.push_str("。请重新打开启动器"),
                                    }
                                    message.set(status);
                                }
                            }
                            break;
                        }
                        Ok(QrStatus::Expired) => {
                            message.set("二维码已过期，请刷新二维码".into());
                        }
                        Ok(QrStatus::Cancelled) => {
                            self.cancel_scan();
                            break;
                        }
                        Err(error) => message.set(error),
                    }
                    scan.set(Scan::Retry);
                    break;
                }
            }
            Err(error) => {
                scan.set(Scan::Retry);
                message.set(error);
            }
        }
    }
}

async fn verify_authorization(
    environment: &dyn LauncherEnvironment,
    authorization: &Authorization,
    expected: Option<&VerifiedAccount>,
) -> Result<String, String> {
    if expected.is_some_and(|account| account.account_id != authorization.account_id) {
        return Err("扫码账号与待更新账号不一致，请使用原账号重新扫码".into());
    }
    let uid = environment.verify_uid(authorization.clone()).await?;
    crate::profile::validate_uid(&uid).map_err(|_| "米游社返回的游戏 UID 无效")?;
    if expected.is_some_and(|account| account.uid != uid) {
        return Err("验证所得游戏 UID 与原授权不一致，已保留原授权".into());
    }
    Ok(uid)
}
