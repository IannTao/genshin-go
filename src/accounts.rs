//! Account authorization UI; external effects use the application's environment.
use crate::launcher::{LauncherEnvironment, button};
use crate::profile::{Profile, VerifiedAccount};
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
/// Contains a `MiHoYo` account identifier and session Cookie in memory.
#[derive(Clone)]
pub struct Authorization {
    /// Identifies a `MiHoYo` account rather than a game character.
    pub account_id: String,
    /// Contains the Cookie request header and must never be logged.
    pub cookie_header: String,
}

impl Authorization {
    /// Validates the account identifier and bounded Cookie payload.
    pub fn validate(&self) -> Result<(), String> {
        if self.account_id.is_empty()
            || self.account_id.len() > 20
            || !self.account_id.bytes().all(|c| c.is_ascii_digit())
            || self.cookie_header.is_empty()
            || self.cookie_header.len() > 8192
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

/// Renders `MiHoYo` authorization controls in the Rust root.
pub(crate) fn accounts(
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
) -> impl IntoElement {
    let message = signal(String::new());
    let scan = signal(Scan::Idle);
    let generation = signal(0_u64);
    let checking = signal(false);
    let check_environment = environment.clone();
    let start = Rc::new(move || {
        start_authorization(environment.clone(), generation, scan, message, profile);
    });
    let refresh = start.clone();
    view()
        .gap_2()
        .child(text("米哈游账号"))
        .child(dynamic(move || {
            let start = start.clone();
            let Some(account) = profile.get().account else {
                return view()
                    .child(text("尚未保存米哈游账号"))
                    .child(button("扫码添加米哈游账号", move || start()))
                    .into_element();
            };
            let id = account.account_id.clone();
            let environment = check_environment.clone();
            let remove = environment.clone();
            view()
                .gap_2()
                .child(text(format!("米哈游账号：{id}")))
                .child(text(format!("已验证游戏 UID：{}", account.uid)))
                .child(button(format!("重新扫码：{id}"), move || start()))
                .child(button(format!("移除账号：{id}"), move || match remove
                    .remove_authorization(&account)
                {
                    Ok(state) => {
                        generation.update(|g| *g += 1);
                        scan.set(Scan::Idle);
                        profile.set(state);
                        message.set("已移除米游社授权，已保留 UID 数据".into());
                    }
                    Err(error) => message.set(format!("移除账号失败：{error}")),
                }))
                .child(button(format!("检查授权：{id}"), move || {
                    if checking.get_untracked()
                        || !matches!(scan.get_untracked(), Scan::Idle | Scan::Retry)
                    {
                        return;
                    }
                    checking.set(true);
                    scan.set(Scan::Idle);
                    let current = generation.get_untracked();
                    let environment = environment.clone();
                    let id = id.clone();
                    message.set(format!("正在检查账号 {id} 授权…"));
                    spawn(async move {
                        let result = environment.check_authorization(id.clone()).await;
                        checking.set(false);
                        if generation.get_untracked() == current {
                            message.set(match result {
                                Ok(true) => format!("账号 {id} 米游社授权有效"),
                                Ok(false) => format!("账号 {id} 授权已失效，请重新扫码"),
                                Err(error) => error,
                            });
                        }
                    });
                }))
                .into_element()
        }))
        .child(text(move || message.get()))
        .child(dynamic(move || match scan.get() {
            Scan::Ready(url) => Some(qr_element(&url)),
            Scan::Idle | Scan::Creating | Scan::Verifying | Scan::Retry => None,
        }))
        .child(when(
            move || !matches!(scan.get(), Scan::Idle),
            move || {
                let refresh = refresh.clone();
                view()
                    .gap_2()
                    .child(button("刷新二维码", move || refresh()))
                    .child(button("取消扫码", move || {
                        generation.update(|g| *g += 1);
                        scan.set(Scan::Idle);
                        message.set("已取消扫码".into());
                    }))
            },
        ))
}

fn start_authorization(
    environment: Rc<dyn LauncherEnvironment>,
    generation: Signal<u64>,
    scan: Signal<Scan>,
    message: Signal<String>,
    profile: Signal<Profile>,
) {
    generation.update(|g| *g += 1);
    let current = generation.get_untracked();
    let expected_account = profile.get_untracked().account;
    scan.set(Scan::Creating);
    message.set("正在获取二维码…".into());
    spawn(async move {
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
                        Ok(QrStatus::Cancelled) => message.set("已取消扫码".into()),
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
    });
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
