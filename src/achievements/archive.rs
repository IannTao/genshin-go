use crate::archive_json::Object;
use crate::profile::validate_uid;
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Number, json};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

const VERSION: &str = "v1.1";
const UNKNOWN_TIME: i64 = 253_402_271_999;

#[derive(Deserialize)]
#[serde(untagged)]
enum Uid {
    Text(String),
    Number(u64),
}

#[derive(Deserialize)]
struct Info {
    uiaf_version: String,
    #[serde(rename = "export_app")]
    _export_app: String,
    #[serde(rename = "export_app_version")]
    _export_app_version: Option<String>,
    #[serde(rename = "export_timestamp")]
    _export_timestamp: Option<serde_json::Number>,
    uid: Option<Uid>,
}

#[derive(Deserialize)]
struct RawArchive {
    info: Object<Info>,
    list: Vec<Object<Record>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(u8)]
#[serde(into = "u8")]
enum Status {
    Invalid = 0,
    Unfinished = 1,
    Finished = 2,
    Claimed = 3,
}

impl FromStr for Status {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "0" => Ok(Self::Invalid),
            "1" => Ok(Self::Unfinished),
            "2" => Ok(Self::Finished),
            "3" => Ok(Self::Claimed),
            _ => Err("UIAF 成就状态必须为 0、1、2 或 3"),
        }
    }
}

impl From<Status> for u8 {
    fn from(status: Status) -> Self {
        status as Self
    }
}

/// Stores the four UIAF achievement fields with their original business meaning.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(super) struct Record {
    /// Identifies the achievement in the game.
    #[serde(deserialize_with = "whole_number")]
    pub(super) id: u32,
    #[serde(deserialize_with = "whole_number")]
    current: u32,
    #[serde(deserialize_with = "whole_number")]
    status: Status,
    #[serde(default = "unknown_time", deserialize_with = "whole_number")]
    timestamp: i64,
}

fn unknown_time() -> i64 {
    // The UIGF-org verifier also accepts files that omit the time.
    UNKNOWN_TIME
}

fn whole_number<'de, D, T>(decoder: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let number = Number::deserialize(decoder)?;
    let text = if let Some(decimal) = number.as_f64().filter(|_| number.is_f64()) {
        if decimal.fract() != 0.0 || decimal.abs() > 9_007_199_254_740_991.0 {
            return Err(de::Error::custom("UIAF 成就字段必须为可精确表示的整数"));
        }
        format!("{decimal:.0}")
    } else {
        number.to_string()
    };
    text.parse().map_err(de::Error::custom)
}

impl Record {
    /// Indicates whether the achievement is finished or its reward is claimed.
    pub(super) fn completed(self) -> bool {
        self.status >= Status::Finished
    }

    /// Formats the saved achievement status and current progress.
    pub(super) fn label(self) -> String {
        let status = match self.status {
            Status::Invalid => "无效",
            Status::Unfinished => "未完成",
            Status::Finished => "已完成",
            Status::Claimed => "已领取",
        };
        format!("成就 {} · {status} · 进度 {}", self.id, self.current)
    }

    /// Preserves stronger status, known completion time, and the greatest progress.
    pub(super) fn merge(self, other: Self) -> Self {
        let timestamp = if (self.completed() && other.completed()) || self.status == other.status {
            // Prefer a known time, then the earliest one, regardless of import order.
            std::cmp::min_by_key(self.timestamp, other.timestamp, |time| {
                (matches!(*time, 0 | UNKNOWN_TIME), *time)
            })
        } else if self.status > other.status {
            self.timestamp
        } else {
            other.timestamp
        };
        Self {
            id: self.id,
            current: self.current.max(other.current),
            status: self.status.max(other.status),
            timestamp,
        }
    }
}

/// Holds validated UIAF records grouped by achievement ID.
#[derive(PartialEq, Eq)]
pub(super) struct Archive {
    /// Identifies the source UID when the exporter supplies the optional extension.
    pub(super) uid: Option<String>,
    /// Contains one merged record per achievement.
    pub(super) records: BTreeMap<u32, Record>,
    /// Counts additional identical rows in the input file.
    pub(super) duplicates: usize,
    /// Counts additional differing rows for the same achievement in the input file.
    pub(super) conflicts: usize,
}

impl Archive {
    /// Reads a UIAF v1.1 JSON file and combines repeated achievement IDs.
    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|error| format!("无法读取 UIAF 文件：{error}"))?;
        let Object(RawArchive { info, list }) =
            serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
                .map_err(|error| format!("UIAF JSON 无效：{error}"))?;
        if info.0.uiaf_version != VERSION {
            return Err(format!(
                "仅支持 UIAF {VERSION}，文件为 {}",
                info.0.uiaf_version
            ));
        }
        let uid = info.0.uid.map(|uid| match uid {
            Uid::Text(uid) => uid,
            Uid::Number(uid) => uid.to_string(),
        });
        if let Some(uid) = &uid {
            validate_uid(uid).map_err(|_| "UIAF 文件声明的游戏 UID 无效")?;
        }
        let mut records = BTreeMap::<u32, Record>::new();
        let mut seen = HashSet::new();
        let mut duplicates = 0;
        let mut conflicts = 0;
        for Object(record) in list {
            if !seen.insert(record) {
                duplicates += 1;
                continue;
            }
            records
                .entry(record.id)
                .and_modify(|existing| {
                    conflicts += 1;
                    *existing = existing.merge(record);
                })
                .or_insert(record);
        }
        Ok(Self {
            uid,
            records,
            duplicates,
            conflicts,
        })
    }
}

/// Encodes UIAF v1.1 with optional UID metadata for unambiguous reimport.
pub(super) fn encode(uid: &str, records: &[Record]) -> Result<Vec<u8>, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    serde_json::to_vec_pretty(&json!({
        "info": {
            "export_app": "Genshin-Go",
            "export_app_version": env!("CARGO_PKG_VERSION"),
            "export_timestamp": timestamp,
            "uiaf_version": VERSION,
            "uid": uid,
        },
        "list": records,
    }))
    .map_err(|error| error.to_string())
}
