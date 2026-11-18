//! Windows adapter for the UIGF Passport SDK.
use crate::accounts::{Authorization, QrSession, QrStatus};
use azure_core::http::{
    Context, Request, Response,
    policies::{Policy, PolicyResult},
};
use serde_json::Value;
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uigf_mihoyo::{
    PassportClient, PassportClientOptions,
    models::{JsonObject, QrLoginCreateRequest, QrLoginStatusRequest},
    v4::PassportV4Client,
};

const PASSPORT: &str = "https://passport-api.mihoyo.com";
const APP_ID: &str = "ddxf5dufpuyo";

#[derive(Debug)]
struct PassportHeaders(String);

#[async_trait::async_trait]
impl Policy for PassportHeaders {
    async fn send(
        &self,
        ctx: &Context,
        request: &mut Request,
        next: &[Arc<dyn Policy>],
    ) -> PolicyResult {
        request.insert_header("x-rpc-app_id", APP_ID);
        request.insert_header("x-rpc-device_id", self.0.clone());
        request.insert_header("x-rpc-client_type", "3");
        next[0].send(ctx, request, &next[1..]).await
    }
}

fn client(device: &str) -> Result<PassportClient, String> {
    let mut options = PassportClientOptions::default();
    options
        .client_options
        .per_call_policies
        .push(Arc::new(PassportHeaders(device.into())));
    PassportClient::with_no_credential(PASSPORT, Some(options))
        .map_err(|_| "无法初始化米游社服务".into())
}

fn device_id() -> Result<String, String> {
    Ok(format!(
        "{:?}",
        windows::core::GUID::new().map_err(|_| "无法生成设备标识")?
    ))
}

fn run<T>(future: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "无法启动授权服务")?
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(30), future)
                .await
                .map_err(|_| "米游社服务超时，请重试")?
        })
}

fn response<T>(result: azure_core::Result<Response<T>>) -> Result<(u16, Value), String> {
    let reply = result.map_err(|_| "网络失败：无法连接米游社服务，请重试")?;
    let status = u16::from(reply.status());
    let body = reply.into_body();
    let bytes: &[u8] = body.as_ref();
    if bytes.len() > 64 * 1024 {
        return Err("米游社响应过大".into());
    }
    Ok((
        status,
        serde_json::from_slice(bytes).map_err(|_| "米游社响应格式异常")?,
    ))
}

fn success(value: &Value) -> Result<(), String> {
    match code(value)? {
        0 => Ok(()),
        n => Err(format!("米游社服务失败（服务代码 {n}）")),
    }
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
/// Creates an App QR login ticket.
pub(crate) fn create() -> Result<QrSession, String> {
    run(async {
        let device = device_id()?;
        let client = client(&device)?.get_passport_qr_login_api_client();
        let body = QrLoginCreateRequest {
            app_id: Some(APP_ID.into()),
            device: Some(device.clone()),
            ..Default::default()
        };
        let (_, result) = response(
            client
                .create(body.try_into().map_err(|_| "无法创建扫码请求")?, None)
                .await,
        )?;
        success(&result)?;
        let url = nonempty(&result["data"]["url"])?;
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
            ticket: nonempty(&result["data"]["ticket"])?,
            device,
        })
    })
}

/// Polls the App ticket and exchanges its confirmed session for account cookies.
pub(crate) fn poll(qr: &QrSession) -> Result<QrStatus, String> {
    run(async {
        if SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "系统时间异常")?
            .as_secs()
            >= deadline(&qr.url)?
        {
            return Ok(QrStatus::Expired);
        }
        let passport = client(&qr.device)?;
        let body = QrLoginStatusRequest {
            app_id: Some(APP_ID.into()),
            device: Some(qr.device.clone()),
            ticket: Some(qr.ticket.clone()),
            ..Default::default()
        };
        let (_, result) = response(
            passport
                .get_passport_qr_login_api_client()
                .get_status(body.try_into().map_err(|_| "无法创建扫码请求")?, None)
                .await,
        )?;
        match code(&result)? {
            -3501 => return Ok(QrStatus::Expired),
            -3505 => return Ok(QrStatus::Cancelled),
            _ => success(&result)?,
        }
        let data = &result["data"];
        match data["status"].as_str() {
            Some("Created") => Ok(QrStatus::Waiting),
            Some("Scanned") => Ok(QrStatus::Scanned),
            Some("Confirmed") => Ok(QrStatus::Confirmed(exchange(&passport, data).await?)),
            _ => Err("扫码服务返回未知状态，请重新扫码".into()),
        }
    })
}

fn cookie_value(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 4096
                && s.bytes()
                    .all(|b| b.is_ascii_graphic() && !matches!(b, b';' | b',' | b'"' | b'\\'))
        })
        .ok_or_else(|| "服务未返回有效的授权字段".into())
}

async fn exchange(passport: &PassportClient, data: &Value) -> Result<Authorization, String> {
    let aid = cookie_value(&data["user_info"]["aid"])?;
    let mid = cookie_value(&data["user_info"]["mid"])?;
    let stoken = data["tokens"]
        .as_array()
        .and_then(|tokens| tokens.iter().find(|t| t["token_type"] == 1))
        .ok_or("扫码确认未返回 SToken")?;
    let session = format!("mid={mid}; stoken={}", cookie_value(&stoken["token"])?);
    let tokens = passport.get_passport_account_token_api_client();
    let (_, cookie) = response(
        tokens
            .get_cookie_account_info_by_s_token(session.clone(), None)
            .await,
    )?;
    success(&cookie)?;
    let (_, login) = response(tokens.get_l_token_by_stoken(session, None).await)?;
    success(&login)?;
    authorization_from_exchange(aid, &cookie["data"], &login["data"])
}

fn authorization_from_exchange(
    aid: &str,
    cookie: &Value,
    login: &Value,
) -> Result<Authorization, String> {
    if cookie_value(&cookie["uid"])? != aid {
        return Err("扫码账号与 Cookie 账号不一致".into());
    }
    let cookie = cookie_value(&cookie["cookie_token"])?;
    let login = cookie_value(&login["ltoken"])?;
    let authorization = Authorization {
        account_id: aid.into(),
        cookie_header: format!(
            "account_id={aid}; cookie_token={cookie}; ltuid={aid}; ltoken={login}"
        ),
    };
    authorization.validate()?;
    Ok(authorization)
}

/// Checks both saved cookies with the SDK session clients.
pub(crate) fn check(authorization: &Authorization) -> Result<bool, String> {
    authorization.validate()?;
    run(async {
        let device = device_id()?;
        let v4 = PassportV4Client::with_no_credential("https://passport-api-v4.mihoyo.com", None)
            .map_err(|_| "无法初始化授权检查")?;
        let body = JsonObject::default()
            .try_into()
            .map_err(|_| "无法创建授权检查请求")?;
        let ltoken = v4
            .get_passport_v4_session_api_client()
            .verify_l_token(authorization.cookie_header.clone(), body, None)
            .await;
        verify("verifyLtoken", response(ltoken)?)?;
        let cookie = client(&device)?
            .get_passport_web_session_api_client()
            .verify_cookie_token(authorization.cookie_header.clone(), None)
            .await;
        verify("verifyCookieToken", response(cookie)?)?;
        Ok(true)
    })
}

fn verify(action: &str, (status, value): (u16, Value)) -> Result<(), String> {
    if std::env::var_os("GENSHIN_GO_TRACE_PASSPORT").is_some() {
        trace_session_response(action, status, &value);
    }
    match code(&value)? {
        0 => Ok(()),
        n => Err(format!(
            "暂时无法验证授权（服务代码 {n}）；可重试或重新扫码"
        )),
    }
}
fn trace_session_response(action: &str, status: u16, value: &Value) {
    let message = match value.get("message").and_then(Value::as_str) {
        Some("OK") => "OK",
        Some("登录状态失效，请重新登录") => "登录状态失效，请重新登录",
        Some(_) => "<redacted>",
        None => "<missing or non-string>",
    };
    let data = match value.get("data") {
        None => "missing",
        Some(Value::Null) => "null",
        Some(Value::Object(_)) => "object",
        Some(Value::Array(_)) => "array",
        Some(_) => "<redacted>",
    };
    let extra_fields = value.as_object().map_or(0, |fields| {
        fields
            .keys()
            .filter(|key| !matches!(key.as_str(), "retcode" | "message" | "data"))
            .count()
    });
    eprintln!(
        "[passport-session] {action} HTTP {status} retcode={:?} message={message:?} data={data} extra_fields={extra_fields}",
        value["retcode"].as_i64()
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn stores_only_exchanged_cookies_for_the_confirmed_account() -> Result<(), String> {
        let cookie = serde_json::json!({"uid":"123","cookie_token":"cookie"});
        let login = serde_json::json!({"ltoken":"login"});
        let auth = super::authorization_from_exchange("123", &cookie, &login)?;
        assert_eq!(
            auth.cookie_header,
            "account_id=123; cookie_token=cookie; ltuid=123; ltoken=login"
        );
        assert!(super::authorization_from_exchange("124", &cookie, &login).is_err());
        assert!(
            super::authorization_from_exchange(
                "123",
                &cookie,
                &serde_json::json!({"ltoken":"a;b=c"})
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    #[ignore = "requires live Mihoyo service"]
    fn live_unconfirmed_qr() -> Result<(), Box<dyn std::error::Error>> {
        let qr = super::create()?;
        assert!(matches!(super::poll(&qr)?, super::QrStatus::Waiting));
        Ok(())
    }

    #[test]
    #[ignore = "requires the live passport service"]
    fn live_passport_rejects_invalid_tokens() {
        let authorization = super::Authorization {
            account_id: "0".into(),
            cookie_header: "account_id_v2=0; ltoken_v2=invalid; cookie_token_v2=invalid".into(),
        };
        assert_eq!(
            super::check(&authorization),
            Err("暂时无法验证授权（服务代码 -100）；可重试或重新扫码".into())
        );
    }
}
