//! Windows passport QR adapter. Never log tickets or cookie values.
use crate::accounts::{Authorization, QrSession, QrStatus};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
const BASE: &str = "https://passport-api.miyoushe.com/account/ma-cn-passport/web";

fn request(endpoint: &str, body: Value, device: &str) -> Result<(Value, Vec<String>), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(0)
        .build()
        .into();
    let mut response = agent
        .post(endpoint)
        .header("x-rpc-app_id", "bll8iq97cem8")
        .header("x-rpc-device_id", device)
        .send_json(body)
        .map_err(|_| "网络失败：无法连接米游社扫码服务，请重试".to_owned())?;
    let cookies = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|header| header.to_str().ok().map(str::to_owned))
        .collect();
    let value = response
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_json()
        .map_err(|_| "扫码服务返回格式异常，请重试".to_owned())?;
    Ok((value, cookies))
}
fn code(value: &Value) -> Result<i64, String> {
    value["retcode"]
        .as_i64()
        .ok_or_else(|| "扫码服务返回缺少状态码".into())
}
fn nonempty(value: &Value) -> Result<String, String> {
    value
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "扫码服务返回缺少必要字段".into())
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
fn cookies_to_authorization(
    headers: &[String],
    expected_account: &str,
    expected_mid: &str,
) -> Result<Authorization, String> {
    let mut found = std::collections::BTreeMap::new();
    for header in headers {
        let Some((name, value)) = header.split(';').next().and_then(|v| v.split_once('=')) else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if !value.is_empty()
            && matches!(
                name,
                "account_id" | "account_id_v2" | "ltoken_v2" | "cookie_token_v2" | "ltmid_v2"
            )
        {
            if value.bytes().any(|b| !b.is_ascii_graphic() || b == b';') {
                return Err("Cookie 内容无效".into());
            }
            found.insert(name.to_owned(), value.to_owned());
        }
    }
    let id = found
        .get("account_id_v2")
        .or_else(|| found.get("account_id"))
        .ok_or("扫码确认未返回账号标识")?
        .clone();
    if id != expected_account {
        return Err("扫码账号与 Cookie 账号不一致".into());
    }
    if expected_mid.is_empty()
        || expected_mid.len() > 128
        || !expected_mid
            .bytes()
            .all(|b| b.is_ascii_graphic() && b != b';')
    {
        return Err("扫码确认未返回有效的 MiHoYo ID".into());
    }
    if found.get("ltmid_v2").is_some_and(|mid| mid != expected_mid) {
        return Err("扫码账号与 Cookie 的 MiHoYo ID 不一致".into());
    }
    found
        .entry("ltmid_v2".into())
        .or_insert_with(|| expected_mid.into());
    if !found.contains_key("ltoken_v2") || !found.contains_key("cookie_token_v2") {
        return Err("扫码确认未返回完整米游社授权 Cookie".into());
    }
    let authorization = Authorization {
        account_id: id.clone(),
        cookie_header: found
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; "),
    };
    authorization.validate()?;
    Ok(authorization)
}

/// Creates a passport QR login session with a fresh device identifier.
pub fn create() -> Result<QrSession, String> {
    let device = format!(
        "{:?}",
        windows::core::GUID::new().map_err(|_| "无法生成扫码设备标识")?
    );
    let (response, _) = request(&format!("{BASE}/createQRLogin"), json!({}), &device)?;
    let retcode = code(&response)?;
    if retcode != 0 {
        return Err(format!("创建二维码失败（服务代码 {retcode}）"));
    }
    let url = nonempty(&response["data"]["url"])?;
    let parsed = url::Url::parse(&url).map_err(|_| "二维码地址无效")?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("user.mihoyo.com")
        || parsed.path() != "/login-platform/mobile.html"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
    {
        return Err("扫码服务返回不支持的二维码地址".into());
    }
    deadline(&url)?;
    Ok(QrSession {
        url,
        ticket: nonempty(&response["data"]["ticket"])?,
        device,
    })
}
/// Polls the passport service for the current QR login state.
pub fn poll(qr: &QrSession) -> Result<QrStatus, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "系统时间异常")?
        .as_secs();
    if now >= deadline(&qr.url)? {
        return Ok(QrStatus::Expired);
    }
    let (response, cookies) = request(
        &format!("{BASE}/queryQRLoginStatus"),
        json!({"ticket":qr.ticket}),
        &qr.device,
    )?;
    match code(&response)? {
        -3501 => return Ok(QrStatus::Expired),
        -3505 => return Ok(QrStatus::Cancelled),
        0 => (),
        value => return Err(format!("扫码服务失败（服务代码 {value}），请重新扫码")),
    }
    match response["data"]["status"].as_str() {
        Some("Created") => Ok(QrStatus::Waiting),
        Some("Scanned") => Ok(QrStatus::Scanned),
        Some("Confirmed") => {
            let id = nonempty(&response["data"]["user_info"]["aid"])?;
            let mid = nonempty(&response["data"]["user_info"]["mid"])?;
            Ok(QrStatus::Confirmed(cookies_to_authorization(
                &cookies, &id, &mid,
            )?))
        }
        _ => Err("扫码服务返回未知状态，请重新扫码".into()),
    }
}
/// Checks whether a saved Cookie is accepted by a `MiHoYo` service.
pub fn check(authorization: &Authorization) -> Result<bool, String> {
    authorization.validate()?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(0)
        .build()
        .into();
    let mut response = agent
        .get("https://api-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie")
        .query("game_biz", "hk4e_cn")
        .header("cookie", &authorization.cookie_header)
        .call()
        .map_err(|_| "网络失败：无法验证米游社授权".to_owned())?;
    let value: Value = response
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_json()
        .map_err(|_| "授权检查返回格式异常".to_owned())?;
    match code(&value)? {
        0 => Ok(true),
        // This endpoint also expects signed client headers. Its rejection alone
        // cannot distinguish an expired Cookie from a missing request signature.
        -100 | -111 => Err("暂时无法验证授权：服务拒绝了检查请求；可以重新扫码".into()),
        value => Err(format!(
            "暂时无法验证授权（服务代码 {value}）；可重试或重新扫码"
        )),
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn extracts_only_expected_cookie_fields() -> Result<(), Box<dyn std::error::Error>> {
        let headers = vec![
            "account_id_v2=10001; Path=/".into(),
            "ltoken_v2=secret; HttpOnly".into(),
            "cookie_token_v2=token; Secure".into(),
            "other=ignored; Path=/".into(),
        ];
        let auth = super::cookies_to_authorization(&headers, "10001", "mH0-20001")?;
        assert_eq!(auth.account_id, "10001");
        assert!(!auth.cookie_header.contains("other="));
        assert!(auth.cookie_header.contains("ltmid_v2=mH0-20001"));
        assert!(super::cookies_to_authorization(&headers, "10002", "mH0-20001").is_err());
        Ok(())
    }
    #[test]
    #[ignore = "requires live Mihoyo service"]
    fn live_unconfirmed_qr() -> Result<(), Box<dyn std::error::Error>> {
        let qr = super::create()?;
        assert!(matches!(super::poll(&qr)?, super::QrStatus::Waiting));
        Ok(())
    }
}
