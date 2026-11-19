//! Windows client for the `MiYouShe` community sign-in service.
use crate::{
    account_service::APP_ID,
    account_store,
    community_sign_in::{Clock, Completion, Failure},
    profile::VerifiedAccount,
};
use reqwest::{Client, RequestBuilder, StatusCode, header};
use serde_json::Value;
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use windows::Win32::System::WindowsProgramming::{
    QueryInterruptTimePrecise, QueryUnbiasedInterruptTimePrecise,
};

const HOST: &str = "https://bbs-api.miyoushe.com";
const SIGN_BODY: &str = r#"{"gids":2}"#;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REPLY_BYTES: usize = 64 * 1024;

fn unix_seconds() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs())
}

fn service_day(seconds: u64) -> u64 {
    (seconds + 8 * 3600) / 86_400
}

/// Samples Beijing time and Windows suspend time without using the user's time zone.
#[expect(
    unsafe_code,
    reason = "Windows clock queries take no arguments and return counts by value"
)]
pub(crate) fn clock() -> Option<Clock> {
    let day = service_day(unix_seconds()?);
    // These clocks share the same epoch and tick size. Only the first includes sleep.
    let ticks =
        unsafe { QueryInterruptTimePrecise().saturating_sub(QueryUnbiasedInterruptTimePrecise()) };
    Some(Clock {
        day,
        suspended: Duration::from_nanos(ticks.saturating_mul(100)),
    })
}

fn transport_error(error: &reqwest::Error) -> Failure {
    if error.is_timeout() {
        Failure::Timeout
    } else {
        Failure::Other("无法连接米游社".into())
    }
}

async fn request(request: RequestBuilder, cookie: &str) -> Result<Value, Failure> {
    let mut cookie = header::HeaderValue::from_str(cookie).map_err(|_| Failure::Authorization)?;
    cookie.set_sensitive(true);
    let mut reply = request
        .header(header::COOKIE, cookie)
        .send()
        .await
        .map_err(|error| transport_error(&error))?;
    let status = reply.status();
    match status {
        StatusCode::UNAUTHORIZED => return Err(Failure::Authorization),
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => return Err(Failure::Timeout),
        status if !status.is_success() => {
            return Err(Failure::Other(format!(
                "米游社请求失败（HTTP {}）",
                status.as_u16()
            )));
        }
        _ => {}
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = reply
        .chunk()
        .await
        .map_err(|error| transport_error(&error))?
    {
        if bytes.len() + chunk.len() > MAX_REPLY_BYTES {
            return Err(Failure::Other("签到响应过大".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid_reply())
}

fn invalid_reply() -> Failure {
    Failure::Other("米游社签到响应格式异常".into())
}

fn reply_code(reply: &Value) -> Result<i64, Failure> {
    let code = reply["retcode"].as_i64().ok_or_else(invalid_reply)?;
    let verification = reply["message"]
        .as_str()
        .is_some_and(|message| message.contains("验证码") || message.contains("人机验证"));
    match code {
        -100 => Err(Failure::Authorization),
        1034 => Err(Failure::Verification),
        code if code != 0 && verification => Err(Failure::Verification),
        code => Ok(code),
    }
}

fn rejected(code: i64) -> Failure {
    Failure::Other(format!("米游社拒绝签到请求（服务代码 {code}）"))
}

fn already_signed(reply: &Value) -> Result<bool, Failure> {
    let code = reply_code(reply)?;
    if code != 0 {
        return Err(rejected(code));
    }
    let states = reply["data"]["states"]
        .as_array()
        .ok_or_else(invalid_reply)?;
    Ok(states.iter().any(|mission| {
        (mission["mission_id"].as_u64() == Some(58)
            || mission["mission_key"].as_str() == Some("continuous_sign"))
            && (mission["is_get_award"].as_bool() == Some(true)
                || mission["happened_times"]
                    .as_u64()
                    .is_some_and(|count| count > 0))
    }))
}

fn completion(reply: &Value) -> Result<Completion, Failure> {
    let code = reply_code(reply)?;
    if code == 0 {
        return Ok(Completion::Signed);
    }
    if reply["message"].as_str().is_some_and(|message| {
        ["已签到", "已经签到", "请勿重复签到"]
            .iter()
            .any(|phrase| message.contains(phrase))
    }) {
        return Ok(Completion::AlreadySigned);
    }
    Err(rejected(code))
}

fn dynamic_secret(timestamp: u64, nonce: u32) -> String {
    let digest = md5::compute(format!(
        "salt=t0qEgfub6cvueAPgR5m9aQWWVciEer7v&t={timestamp}&r={nonce}&b={SIGN_BODY}&q="
    ));
    format!("{timestamp},{nonce},{digest:x}")
}

async fn read_account<T: Send + 'static>(
    read: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, Failure> {
    tokio::task::spawn_blocking(read)
        .await
        .map_err(|_| Failure::Other("无法读取本机签到授权".into()))?
        .map_err(Failure::Other)
}

/// Checks the remote task state and submits one sign-in for the verified account when needed.
pub(crate) async fn sign_in(
    path: PathBuf,
    account: VerifiedAccount,
    day: u64,
) -> Result<Completion, Failure> {
    let grant_path = path.clone();
    let expected = account.clone();
    let grant = read_account(move || account_store::load_community_grant(&grant_path, &expected))
        .await?
        .ok_or(Failure::Authorization)?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| Failure::Other("无法初始化米游社签到".into()))?;
    let state = request(
        client
            .get(format!(
                "{HOST}/apihub/wapi/getUserMissionsState?point_sn=myb"
            ))
            .header(header::ORIGIN, "https://webstatic.mihoyo.com")
            .header(header::REFERER, "https://webstatic.mihoyo.com/"),
        &grant.web_cookie,
    )
    .await?;
    if already_signed(&state)? {
        return Ok(Completion::AlreadySigned);
    }
    let cookie = grant.community_cookie.ok_or(Failure::Authorization)?;
    if !read_account(move || account_store::is_current_authorization(&path, &account)).await? {
        return Err(Failure::Authorization);
    }
    let timestamp = unix_seconds().ok_or_else(|| Failure::Other("系统时间异常".into()))?;
    if service_day(timestamp) != day {
        return Err(Failure::Other("签到日期已变更".into()));
    }
    let nonce =
        windows::core::GUID::new().map_err(|_| Failure::Other("无法生成签到请求随机数".into()))?;
    let result = request(
        client
            .post(format!("{HOST}/apihub/app/api/signIn"))
            .header(header::CONTENT_TYPE, "application/json; charset=UTF-8")
            .header(header::REFERER, "https://app.mihoyo.com")
            .header(header::USER_AGENT, "okhttp/4.9.3")
            .header("x-rpc-client_type", "2")
            .header("x-rpc-app_version", "2.106.2")
            .header("x-rpc-channel", "miyousheluodi")
            .header("x-rpc-verify_key", APP_ID)
            .header("x-rpc-device_id", grant.device)
            .header(
                "DS",
                dynamic_secret(timestamp, 100_001 + nonce.data1 % 100_000),
            )
            .body(SIGN_BODY),
        &cookie,
    )
    .await?;
    completion(&result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn beijing_day_changes_at_1600_utc() {
        assert_eq!(service_day(57_599), 0);
        assert_eq!(service_day(57_600), 1);
        assert_eq!(service_day(143_999), 1);
    }

    #[test]
    fn only_the_community_sign_in_mission_marks_the_day_complete() {
        let reading = json!({"mission_id": 59, "is_get_award": true, "happened_times": 3});
        assert!(matches!(
            already_signed(&json!({"retcode":0,"data":{"states":[reading]}})),
            Ok(false)
        ));
        let sign_in = json!({"mission_key":"continuous_sign","happened_times":1});
        assert!(matches!(
            already_signed(&json!({"retcode":0,"data":{"states":[sign_in]}})),
            Ok(true)
        ));
        assert!(already_signed(&json!({"retcode":0,"data":null})).is_err());
    }

    #[test]
    fn reports_only_explicit_completions_and_blocks_on_authorization_or_verification() {
        assert!(matches!(
            completion(&json!({"retcode":0})),
            Ok(Completion::Signed)
        ));
        assert!(matches!(
            completion(&json!({"retcode":-1,"message":"今日已签到"})),
            Ok(Completion::AlreadySigned)
        ));
        assert!(matches!(
            completion(&json!({"retcode":-100})),
            Err(Failure::Authorization)
        ));
        assert!(matches!(
            completion(&json!({"retcode":1034})),
            Err(Failure::Verification)
        ));
        assert!(matches!(
            completion(&json!({"retcode":-10001})),
            Err(Failure::Other(_))
        ));
    }
}
