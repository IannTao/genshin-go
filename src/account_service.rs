//! Windows service adapter. Responses and transport errors never expose tokens.
use crate::accounts::{Authorization, QrSession, QrStatus};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const QR_BASE: &str = "https://hk4e-sdk.mihoyo.com/hk4e_cn/combo/panda/qrcode";

fn post(endpoint: &str, body: Value, passport: bool) -> Result<Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(0)
        .build()
        .into();
    let mut request = agent.post(endpoint);
    if passport {
        request = request.header("x-rpc-app_id", "bll8iq97cem8");
    }
    let mut response = request
        .send_json(body)
        .map_err(|_| "网络失败：无法连接米哈游服务，请重试".to_owned())?;
    response
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_json()
        .map_err(|_| "服务返回格式异常，请重试".into())
}

fn code(value: &Value) -> Result<i64, String> {
    value["retcode"]
        .as_i64()
        .ok_or_else(|| "服务返回缺少状态码".into())
}

fn nonempty(value: &Value) -> Result<String, String> {
    value
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "服务返回缺少必要字段".into())
}

pub fn create() -> Result<QrSession, String> {
    // A new identifier per QR session; creation and polling use the same one.
    let device = format!(
        "{:?}",
        unsafe { windows::Win32::System::Com::CoCreateGuid() }
            .map_err(|_| "无法生成扫码设备标识")?
    );
    let response = post(
        &format!("{QR_BASE}/fetch"),
        json!({"app_id":"4", "device":device}),
        false,
    )?;
    let retcode = code(&response)?;
    if retcode != 0 {
        return Err(format!("创建二维码失败（服务代码 {retcode}），请重试"));
    }
    let url = nonempty(&response["data"]["url"])?;
    let parsed = url::Url::parse(&url).map_err(|_| "二维码地址无效")?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("user.mihoyo.com")
        || parsed.path() != "/qr_code_in_game.html"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
    {
        return Err("服务返回了不支持的二维码地址".into());
    }
    let ticket = parsed
        .query_pairs()
        .find(|(key, _)| key == "ticket")
        .map(|(_, value)| value.into_owned())
        .filter(|s| !s.is_empty())
        .ok_or("二维码缺少票据")?;
    // Require a service-supplied deadline so an abandoned session cannot poll forever.
    deadline(&url)?;
    Ok(QrSession {
        url,
        ticket,
        device,
    })
}

fn deadline(url: &str) -> Result<u64, String> {
    url::Url::parse(url)
        .ok()
        .and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key == "expire")
                .and_then(|(_, value)| value.parse().ok())
        })
        .ok_or_else(|| "二维码缺少有效期".into())
}

pub fn poll(qr: QrSession) -> Result<QrStatus, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "系统时间异常")?
        .as_secs();
    if now >= deadline(&qr.url)? {
        return Ok(QrStatus::Expired);
    }
    let response = post(
        &format!("{QR_BASE}/query"),
        json!({"app_id":"4", "device":qr.device, "ticket":qr.ticket}),
        false,
    )?;
    match code(&response)? {
        -106 => return Ok(QrStatus::Expired),
        0 => (),
        -3503 => return Err("扫码服务拒绝请求：设备或网络环境存在风险（-3503）".into()),
        value => return Err(format!("扫码服务失败（服务代码 {value}），请重新扫码")),
    }
    match response["data"]["stat"].as_str() {
        Some("Init") => Ok(QrStatus::Waiting),
        Some("Scanned") => Ok(QrStatus::Scanned),
        Some("Confirmed") => {
            if response["data"]["payload"]["proto"] != "Account" {
                return Err("扫码确认未返回账号授权".into());
            }
            let raw = nonempty(&response["data"]["payload"]["raw"])?;
            let payload: Value = serde_json::from_str(&raw).map_err(|_| "授权返回格式异常")?;
            let authorization = Authorization {
                account_id: nonempty(&payload["uid"])?,
                game_token: nonempty(&payload["token"])?,
            };
            authorization.validate()?;
            Ok(QrStatus::Confirmed(authorization))
        }
        Some("Cancelled") => Ok(QrStatus::Cancelled),
        _ => Err("扫码服务返回未知状态，请重新扫码".into()),
    }
}

pub fn check(authorization: Authorization) -> Result<bool, String> {
    authorization.validate()?;
    let response = post(
        "https://api-takumi.mihoyo.com/account/ma-cn-session/app/getTokenByGameToken",
        json!({"account_id": authorization.account_id.parse::<u64>().map_err(|_| "账号标识无效")?,
            "game_token":authorization.game_token}),
        true,
    )?;
    match code(&response)? {
        0 => {
            if response["data"]["user_info"]["aid"].as_str() != Some(&authorization.account_id)
                || response["data"]["token"]["token"]
                    .as_str()
                    .is_none_or(str::is_empty)
            {
                return Err("授权校验返回不完整或账号不匹配".into());
            }
            // The converted credential is deliberately not retained: T02 stores GameToken only.
            Ok(true)
        }
        -100 => Ok(false),
        value => Err(format!(
            "暂时无法验证授权（服务代码 {value}）；可重试或重新扫码"
        )),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires live Mihoyo service"]
    fn live_unconfirmed_qr() {
        let qr = super::create().unwrap();
        assert!(matches!(super::poll(qr).unwrap(), super::QrStatus::Waiting));
    }
}
