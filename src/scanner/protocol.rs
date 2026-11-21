use crate::{archive_json::Object, profile::validate_uid};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, time::Duration};

/// Describes one revision of the inventory for a verified login.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
    pub session_id: String,
    pub revision: i64,
    pub updated_at: i64,
    data: Object<Export>,
}

#[derive(Deserialize)]
struct Export {
    format: String,
    version: u8,
    source: String,
    metadata: Object<Metadata>,
    characters: Option<Value>,
}

#[derive(Deserialize)]
struct Metadata {
    uid: String,
}

/// Contains one owned character in a specific elemental form.
#[derive(Serialize, Deserialize)]
pub(super) struct Character {
    pub key: String,
    pub element: Option<String>,
    pub level: u8,
    pub ascension: u8,
    pub constellation: u8,
    pub talent: Talent,
}

/// Records the normal attack, skill and burst levels of one character.
#[derive(Serialize, Deserialize)]
pub(super) struct Talent {
    pub auto: u8,
    pub skill: u8,
    pub burst: u8,
}

impl Snapshot {
    /// Returns the UID reported for this login by the scanner.
    pub(super) fn uid(&self) -> &str {
        &self.data.0.metadata.0.uid
    }

    /// Takes the complete roster after validating every character in the array.
    pub(super) fn characters(&mut self) -> Result<Option<Vec<Character>>, String> {
        let Some(value) = self.data.0.characters.take() else {
            return Ok(None);
        };
        let invalid = "GGScanner 的角色列表无效";
        let Value::Array(array) = value else {
            return Err(invalid.into());
        };
        let mut characters = Vec::with_capacity(array.len());
        let mut identities = HashSet::new();
        for item in array {
            if !item.is_object() || !item["talent"].is_object() {
                return Err(invalid.into());
            }
            let character: Character = serde_json::from_value(item).map_err(|_| invalid)?;
            if character.key.trim().is_empty()
                || character
                    .element
                    .as_ref()
                    .is_some_and(|e| e.trim().is_empty())
                || !(1..=100).contains(&character.level)
                || character.ascension > 6
                || character.constellation > 6
                || [
                    character.talent.auto,
                    character.talent.skill,
                    character.talent.burst,
                ]
                .iter()
                .any(|v| !(1..=15).contains(v))
                || !identities.insert((character.key.clone(), character.element.clone()))
            {
                return Err(invalid.into());
            }
            characters.push(character);
        }
        Ok(Some(characters))
    }
}

/// Reads the continuous capture endpoint on the configured loopback port.
pub(super) struct Service {
    client: Client,
    endpoint: String,
}

/// Distinguishes a missing listener from an unusable continuous capture response.
pub(super) enum ReadError {
    Unavailable,
    Failed(String),
}

impl From<&str> for ReadError {
    fn from(error: &str) -> Self {
        Self::Failed(error.to_owned())
    }
}

impl ReadError {
    /// Returns the reason presented by the launcher or connection check.
    pub(super) fn message(self) -> String {
        match self {
            Self::Unavailable => "无法连接 GGScanner，请检查原神管理服务和本机端口".into(),
            Self::Failed(message) => message,
        }
    }
}

impl Service {
    /// Creates a local client with a bounded request timeout.
    pub(super) fn new(port: u16) -> Result<Self, String> {
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .build()
            .map_err(|error| format!("无法初始化扫描连接：{error}"))?;
        Ok(Self {
            client,
            endpoint: format!("http://127.0.0.1:{port}/continuous"),
        })
    }

    /// Returns the current snapshot or an empty result while login is pending.
    pub(super) async fn read(&self) -> Result<Option<Snapshot>, ReadError> {
        let mut response = self
            .client
            .get(&self.endpoint)
            .send()
            .await
            .map_err(|error| {
                if error.is_connect() {
                    ReadError::Unavailable
                } else {
                    "GGScanner 本机端口未返回有效响应，请检查端口是否被占用".into()
                }
            })?;
        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "读取 GGScanner 响应失败")?
        {
            if body.len() + chunk.len() > 16 * 1024 * 1024 {
                return Err("GGScanner 响应过大".into());
            }
            body.extend_from_slice(&chunk);
        }
        if status == StatusCode::OK {
            let Object(snapshot): Object<Snapshot> =
                serde_json::from_slice(&body).map_err(|_| "GGScanner 快照格式无效")?;
            if snapshot.data.0.format != "GOOD" || snapshot.data.0.version != 3 {
                return Err("请选择支持 GOOD v3 的原神管理服务".into());
            }
            if snapshot.data.0.source.trim().is_empty() {
                return Err("GGScanner 快照缺少 GOOD 数据来源".into());
            }
            validate_uid(snapshot.uid()).map_err(|_| "GGScanner 快照缺少可信游戏 UID")?;
            if snapshot.session_id.trim().is_empty()
                || snapshot.revision < 1
                || snapshot.updated_at < 1
            {
                return Err("GGScanner 会话或修订无效".into());
            }
            return Ok(Some(snapshot));
        }
        let error = serde_json::from_slice::<Value>(&body).unwrap_or_default();
        match (status, error["error"].as_str()) {
            (StatusCode::SERVICE_UNAVAILABLE, Some("continuous_capture_no_data")) => Ok(None),
            (StatusCode::CONFLICT, Some("continuous_capture_disabled")) => {
                Err("持续同步已禁用，请在 GGScanner 勾选持续同步并重启原神管理服务".into())
            }
            (StatusCode::SERVICE_UNAVAILABLE, Some("continuous_capture_failed")) => {
                Err("GGScanner 采集失败，请查看扫描器诊断并重启管理服务".into())
            }
            _ => Err(ReadError::Failed(format!(
                "GGScanner 服务错误（HTTP {}），请检查本机端口",
                status.as_u16()
            ))),
        }
    }
}
