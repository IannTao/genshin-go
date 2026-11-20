use crate::profile::validate_uid;
use chrono::{Duration, NaiveDateTime};
use serde::de::{IgnoredAny, MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use std::fmt;
use std::fs;
use std::marker::PhantomData;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

#[derive(Deserialize)]
#[serde(untagged)]
enum TextOrInteger {
    Text(String),
    Integer(i64),
}

impl TextOrInteger {
    fn into_string(self) -> String {
        match self {
            Self::Text(value) => value,
            Self::Integer(value) => value.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct Info {
    #[serde(default, deserialize_with = "when_present")]
    version: Option<String>,
    uigf_version: Option<String>,
    #[serde(
        default,
        rename = "export_timestamp",
        deserialize_with = "when_present"
    )]
    timestamp: Option<TextOrInteger>,
    #[serde(default, rename = "export_app", deserialize_with = "when_present")]
    app: Option<String>,
    #[serde(
        default,
        rename = "export_app_version",
        deserialize_with = "when_present"
    )]
    app_version: Option<String>,
}

fn when_present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}

// A JSON object must not deserialize through Serde's positional struct syntax.
struct Object<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = Object<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<M: MapAccess<'de>>(self, entries: M) -> Result<Self::Value, M::Error> {
                T::deserialize(MapAccessDeserializer::new(entries)).map(Object)
            }
        }

        decoder.deserialize_map(ObjectVisitor(PhantomData))
    }
}

#[derive(Deserialize)]
struct RawArchive {
    info: Object<Info>,
    #[serde(default)]
    hk4e: Vec<Object<RawGroup>>,
    #[serde(default, deserialize_with = "when_present")]
    hkrpg: Option<IgnoredAny>,
    #[serde(default, deserialize_with = "when_present")]
    nap: Option<IgnoredAny>,
    #[serde(default, deserialize_with = "when_present")]
    hk4e_ugc: Option<IgnoredAny>,
}

#[derive(Deserialize)]
struct RawGroup {
    uid: Option<TextOrInteger>,
    timezone: i32,
    #[serde(default, deserialize_with = "when_present")]
    lang: Option<String>,
    list: Vec<Object<Wish>>,
}

/// Identifies wish pools that accumulate separate guarantee counts.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Pool {
    Character,
    Weapon,
    ChronicledAndLightrace,
    Standard,
    Beginner,
}

impl Pool {
    /// Returns the display name for the shared pool.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Character => "角色活动祈愿",
            Self::Weapon => "武器活动祈愿",
            Self::ChronicledAndLightrace => "集录／溯光祈愿",
            Self::Standard => "常驻祈愿",
            Self::Beginner => "新手祈愿",
        }
    }
}

/// Represents the rarity explicitly recorded by the UIGF archive.
#[derive(Clone, Copy)]
pub(super) enum Rarity {
    Three,
    Four,
    Five,
    Unknown,
}

/// Represents a single original game wish with its time normalized to UTC.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct Wish {
    /// Identifies the draw within its UID.
    pub(super) id: String,
    /// Records the UTC date and time of the draw.
    pub(super) time: String,
    item_id: String,
    gacha_type: String,
    uigf_gacha_type: String,
    #[serde(
        default,
        deserialize_with = "when_present",
        skip_serializing_if = "Option::is_none"
    )]
    name: Option<String>,
    #[serde(
        default,
        deserialize_with = "when_present",
        skip_serializing_if = "Option::is_none"
    )]
    rank_type: Option<String>,
    #[serde(
        default,
        deserialize_with = "when_present",
        skip_serializing_if = "Option::is_none"
    )]
    item_type: Option<String>,
    #[serde(
        default,
        deserialize_with = "when_present",
        skip_serializing_if = "Option::is_none"
    )]
    count: Option<String>,
}

impl Wish {
    fn pool_code(&self) -> &str {
        match self.gacha_type.as_str() {
            "400" => "301",
            code => code,
        }
    }

    /// Returns the shared pool for the raw game banner code.
    pub(super) fn pool(&self) -> Option<Pool> {
        Some(match self.pool_code() {
            "100" => Pool::Beginner,
            "200" => Pool::Standard,
            "301" => Pool::Character,
            "302" => Pool::Weapon,
            "500" => Pool::ChronicledAndLightrace,
            _ => return None,
        })
    }

    /// Interprets the archived star rating, preserving missing or invalid values as unknown.
    pub(super) fn rarity(&self) -> Rarity {
        match self.rank_type.as_deref() {
            Some("3") => Rarity::Three,
            Some("4") => Rarity::Four,
            Some("5") => Rarity::Five,
            _ => Rarity::Unknown,
        }
    }

    fn normalize(&mut self, timezone: i32) -> Result<(), String> {
        if self.id.len() > 19 || !is_decimal(&self.id) {
            return Err("id 必须是 1–19 位数字字符串".into());
        }
        self.id = self
            .id
            .parse::<u64>()
            .map_err(|error| error.to_string())?
            .to_string();
        if !is_decimal(&self.item_id) {
            return Err("item_id 必须是数字字符串".into());
        }
        self.pool().ok_or("gacha_type 无效")?;
        if self.uigf_gacha_type != self.pool_code() {
            return Err("uigf_gacha_type 与 gacha_type 不匹配".into());
        }
        let local = NaiveDateTime::parse_from_str(&self.time, TIME_FORMAT)
            .map_err(|_| "time 必须是有效的 YYYY-MM-DD HH:mm:ss")?;
        if local.format(TIME_FORMAT).to_string() != self.time || self.time.len() != 19 {
            return Err("time 必须是有效的 YYYY-MM-DD HH:mm:ss".into());
        }
        let utc = local
            .checked_sub_signed(Duration::hours(i64::from(timezone)))
            .ok_or("time 超出可表示范围")?;
        self.time = utc.format(TIME_FORMAT).to_string();
        if self.time.len() != 19 {
            return Err("time 转换为 UTC 后超出可表示范围".into());
        }
        Ok(())
    }

    /// Checks whether another copy represents the same draw with compatible facts.
    pub(super) fn agrees_with(&self, other: &Self) -> bool {
        fn compatible(left: Option<&str>, right: Option<&str>) -> bool {
            left.zip(right).is_none_or(|(left, right)| left == right)
        }
        self.time == other.time
            && self.item_id == other.item_id
            && self.gacha_type == other.gacha_type
            && compatible(self.count.as_deref(), other.count.as_deref())
            && compatible(self.rank_type.as_deref(), other.rank_type.as_deref())
    }

    /// Fills omitted optional fields from another compatible copy of this draw.
    pub(super) fn fill_missing_fields(&mut self, other: &Self) {
        if self.count.is_none() {
            self.count.clone_from(&other.count);
        }
        if self.rank_type.is_none() {
            self.rank_type.clone_from(&other.rank_type);
        }
        if self.name.is_none() {
            self.name.clone_from(&other.name);
        }
        if self.item_type.is_none() {
            self.item_type.clone_from(&other.item_type);
        }
    }

    /// Formats the draw for browsing, using the item ID when the name is absent.
    pub(super) fn label(&self) -> String {
        let pool = match self.pool() {
            Some(pool) => pool.label(),
            None => self.gacha_type.as_str(),
        };
        let name = self
            .name
            .as_deref()
            .filter(|name| !name.is_empty())
            .map_or_else(|| format!("物品 {}", self.item_id), str::to_owned);
        let rank = self
            .rank_type
            .as_deref()
            .filter(|rank| !rank.is_empty())
            .map_or_else(String::new, |rank| format!(" · {rank} 星"));
        format!("{} · {pool} · {name}{rank}", self.time)
    }
}

/// Holds one source UID's records from a UIGF archive.
#[derive(PartialEq, Eq)]
pub(super) struct Group {
    /// Identifies the source UID when its syntax is recognized.
    pub(super) uid: Option<String>,
    /// Contains the normalized wish records in the group.
    pub(super) list: Vec<Wish>,
}

/// Contains the original game wish groups and summaries of skipped data.
#[derive(PartialEq, Eq)]
pub(super) struct Archive {
    /// Lists the original game wish groups available for import.
    pub(super) groups: Vec<Group>,
    /// Names the unsupported data sections present in the file.
    pub(super) skipped: Vec<&'static str>,
}

impl Archive {
    /// Reads and validates a local UIGF v4.0–v4.2 file.
    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|error| format!("无法读取 UIGF 文件：{error}"))?;
        let json = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
        let Object(RawArchive {
            info: Object(info),
            hk4e,
            hkrpg,
            nap,
            hk4e_ugc,
        }) = serde_json::from_slice(json).map_err(|error| {
            if error.is_data() {
                format!("UIGF 字段无效：{error}")
            } else {
                format!("UIGF JSON 损坏：{error}")
            }
        })?;
        let version = info
            .version
            .as_deref()
            .or(info.uigf_version.as_deref())
            .ok_or("UIGF 缺少有效的 info.version")?;
        if !matches!(version, "v4.0" | "v4.1" | "v4.2") {
            return Err(format!("不支持 UIGF {version}，请选择 v4.0–v4.2 文件"));
        }
        if info.version.is_none() {
            return Err("UIGF 缺少有效的 info.version".into());
        }
        if info
            .timestamp
            .is_none_or(|timestamp| timestamp.into_string().parse::<i64>().is_err())
            || info.app.is_none()
            || info.app_version.is_none()
        {
            return Err("UIGF 导出时间、来源或应用版本无效".into());
        }
        let skipped = [
            (hkrpg, "崩坏：星穹铁道"),
            (nap, "绝区零"),
            (hk4e_ugc, "千星奇域"),
        ]
        .into_iter()
        .filter_map(|(section, label)| section.map(|_| label))
        .collect::<Vec<_>>();
        let groups = hk4e
            .into_iter()
            .enumerate()
            .map(|(index, Object(group))| {
                read_group(group).map_err(|error| format!("原神记录组 {} 无效：{error}", index + 1))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { groups, skipped })
    }
}

fn read_group(group: RawGroup) -> Result<Group, String> {
    if !(-23..=23).contains(&group.timezone) {
        return Err("timezone 必须是有效的整小时偏移".into());
    }
    if group.lang.as_deref().is_some_and(|lang| {
        !matches!(
            lang,
            "de-de"
                | "en-us"
                | "es-es"
                | "fr-fr"
                | "id-id"
                | "it-it"
                | "ja-jp"
                | "ko-kr"
                | "pt-pt"
                | "ru-ru"
                | "th-th"
                | "tr-tr"
                | "vi-vn"
                | "zh-cn"
                | "zh-tw"
        )
    }) {
        return Err("lang 不是 UIGF 支持的语言代码".into());
    }
    let uid = group
        .uid
        .map(TextOrInteger::into_string)
        .filter(|uid| validate_uid(uid).is_ok());
    let list = group
        .list
        .into_iter()
        .enumerate()
        .map(|(index, Object(mut wish))| {
            wish.normalize(group.timezone)
                .map_err(|error| format!("第 {} 条记录：{error}", index + 1))?;
            Ok(wish)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Group { uid, list })
}

fn is_decimal(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

/// Encodes one UID's wishes as a standalone UIGF v4.2 archive.
pub(super) fn encode(uid: &str, list: &[Wish]) -> Result<Vec<u8>, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    serde_json::to_vec_pretty(&json!({
        "info": {
            "version": "v4.2",
            "export_timestamp": timestamp,
            "export_app": "Genshin-Go",
            "export_app_version": env!("CARGO_PKG_VERSION"),
        },
        "hk4e": [{ "uid": uid, "timezone": 0, "list": list }],
    }))
    .map_err(|error| format!("无法生成 UIGF 文件：{error}"))
}
