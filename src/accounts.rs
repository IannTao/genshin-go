//! Account authorization UI; external effects use the application's environment.
use crate::launcher::{LauncherEnvironment, button};
use nsh_core::prelude::*;
use serde::{Deserialize, Serialize};
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
/// Contains a `MiHoYo` account identifier and protected session Cookie data.
#[derive(Clone, Serialize, Deserialize)]
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

/// Renders `MiHoYo` account authorization controls in the Rust root.
pub fn accounts(environment: Rc<dyn LauncherEnvironment>) -> impl IntoElement {
    let message = signal(String::new());
    let session = signal(None::<QrSession>);
    let generation = signal(0_u64);
    let target = signal(None::<String>);
    let checking = signal(false);
    let check_environment = environment.clone();
    let saved = signal(environment.load_accounts().unwrap_or_else(|error| {
        message.set(format!("读取账号失败：{error}"));
        Vec::new()
    }));
    let start_scan = Rc::new(move || {
        start_authorization(
            environment.clone(),
            generation,
            target,
            session,
            message,
            saved,
        );
    });
    let renew_scan = start_scan.clone();
    view()
        .gap_2()
        .child(text("米哈游账号"))
        .child(dynamic(move || {
            let accounts = saved.get();
            if accounts.is_empty() {
                vec![text("尚未保存米哈游账号").into_element()]
            } else {
                accounts
                    .into_iter()
                    .map(|id| {
                        let environment = check_environment.clone();
                        let check_id = id.clone();
                        let start = renew_scan.clone();
                        view()
                            .gap_2()
                            .child(text(format!("米哈游账号：{id}")))
                            .child(button(format!("检查授权：{id}"), move || {
                                if checking.get_untracked() || session.get_untracked().is_some() {
                                    return;
                                }
                                checking.set(true);
                                let current = generation.get_untracked();
                                let environment = environment.clone();
                                let id = check_id.clone();
                                message.set(format!("正在检查账号 {id} 授权…"));
                                spawn(async move {
                                    let result = environment.check_authorization(id.clone()).await;
                                    checking.set(false);
                                    if generation.get_untracked() != current {
                                        return;
                                    }
                                    message.set(match result {
                                        Ok(true) => format!("账号 {id} 授权有效；尚未登录游戏"),
                                        Ok(false) => format!("账号 {id} 授权已失效，请重新扫码"),
                                        Err(error) => error,
                                    });
                                });
                            }))
                            .child(button(format!("重新扫码：{id}"), move || {
                                target.set(Some(id.clone()));
                                start();
                            }))
                            .into_element()
                    })
                    .collect()
            }
        }))
        .child(text(move || message.get()))
        .child(dynamic(move || {
            session.get().map(|qr| match qr_image(&qr.url) {
                Ok(markup) => {
                    nsh_core::element::svg(nsh_core::element::ImageSrc::svg_markup(&markup))
                        .w(px(260.0))
                        .h(px(260.0))
                        .test_id("authorization-qr")
                        .label("米游社授权二维码")
                        .into_element()
                }
                Err(error) => text(error).into_element(),
            })
        }))
        .child(button("扫码添加米哈游账号", {
            let start = start_scan.clone();
            move || {
                target.set(None);
                start();
            }
        }))
        .child(button("刷新二维码", move || start_scan()))
        .child(button("取消扫码", move || {
            generation.update(|g| *g += 1);
            session.set(None);
            message.set("已取消扫码".into());
        }))
}

fn start_authorization(
    environment: Rc<dyn LauncherEnvironment>,
    generation: Signal<u64>,
    target: Signal<Option<String>>,
    session: Signal<Option<QrSession>>,
    message: Signal<String>,
    saved: Signal<Vec<String>>,
) {
    generation.update(|g| *g += 1);
    let current = generation.get_untracked();
    let expected_account = target.get_untracked();
    session.set(None);
    message.set("正在获取二维码…".into());
    spawn(async move {
        let result = environment.create_qr().await;
        if generation.get_untracked() != current {
            return;
        }
        match result {
            Ok(qr) => {
                session.set(Some(qr.clone()));
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
                            if expected_account
                                .as_ref()
                                .is_some_and(|id| *id != authorization.account_id)
                            {
                                message
                                    .set("扫码账号与待更新账号不一致，请使用原账号重新扫码".into());
                                session.set(None);
                                break;
                            }
                            match environment.save_authorization(authorization) {
                                Ok(ids) => {
                                    saved.set(ids);
                                    message.set("授权已保存；尚未登录游戏".into());
                                }
                                Err(error) => {
                                    message.set(format!("保存授权失败：{error}，请重新扫码"));
                                }
                            }
                        }
                        Ok(QrStatus::Expired) => {
                            message.set("二维码已过期，请刷新二维码".into());
                        }
                        Ok(QrStatus::Cancelled) => message.set("已取消扫码".into()),
                        Err(error) => message.set(error),
                    }
                    session.set(None);
                    break;
                }
            }
            Err(error) => message.set(error),
        }
    });
}
