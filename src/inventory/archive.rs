use crate::{archive_json::Object, profile::validate_uid};
use chrono::{DateTime, NaiveDateTime, SecondsFormat};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fmt, fs::File, io::Read, path::Path};

const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Deserialize)]
struct Good {
    format: String,
    version: u8,
    source: String,
    metadata: Option<Object<Metadata>>,
    timestamp: Option<i64>,
    gi_player: Option<Object<LeylinePlayer>>,
    #[serde(default)]
    gi_off: Vec<String>,
    weapons: Option<Vec<Object<Weapon>>>,
    artifacts: Option<Vec<Object<Artifact>>>,
    materials: Option<MaterialCounts>,
}

#[derive(Deserialize)]
struct LeylinePlayer {
    uid: Option<u64>,
}

struct MaterialCounts(BTreeMap<String, u32>);

impl<'de> Deserialize<'de> for MaterialCounts {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct MaterialVisitor;

        impl<'de> Visitor<'de> for MaterialVisitor {
            type Value = MaterialCounts;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object of material quantities")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut entries: M) -> Result<Self::Value, M::Error> {
                let mut materials = BTreeMap::new();
                while let Some((key, quantity)) = entries.next_entry::<String, u32>()? {
                    match materials.entry(key) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(quantity);
                        }
                        std::collections::btree_map::Entry::Occupied(entry) => {
                            return Err(serde::de::Error::custom(format!(
                                "材料标识重复：{}",
                                entry.key()
                            )));
                        }
                    }
                }
                Ok(MaterialCounts(materials))
            }
        }

        decoder.deserialize_map(MaterialVisitor)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    uid: Option<String>,
    captured_at: Option<String>,
}

/// Holds the original GOOD source, destination metadata and complete inventory.
pub(super) struct Archive {
    pub version: u8,
    pub source: String,
    pub uid: Option<String>,
    pub captured_at: Option<String>,
    pub inventory: Inventory,
    pub omitted_categories: Vec<&'static str>,
}

/// Preserves equipment instances and the quantities of a single snapshot.
#[derive(PartialEq)]
pub(super) struct Inventory {
    pub weapons: Vec<Weapon>,
    pub artifacts: Vec<Artifact>,
    pub materials: BTreeMap<String, u32>,
}

/// Describes a weapon instance in a GOOD file.
#[derive(Deserialize, PartialEq, Serialize)]
pub(super) struct Weapon {
    key: String,
    level: u8,
    ascension: u8,
    refinement: u8,
    location: Option<String>,
    lock: Option<bool>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

/// Describes an artifact instance in a GOOD file.
#[derive(Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Artifact {
    set_key: String,
    slot_key: String,
    rarity: u8,
    main_stat_key: String,
    level: u8,
    #[serde(deserialize_with = "substats")]
    substats: Vec<Substat>,
    location: Option<String>,
    lock: Option<bool>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, PartialEq, Serialize)]
struct Substat {
    key: String,
    value: f64,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

fn substats<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Vec<Substat>, D::Error> {
    Ok(Vec::<Object<Substat>>::deserialize(decoder)?
        .into_iter()
        .map(|Object(value)| value)
        .collect())
}

/// Keeps the supplied time and its timezone-independent sort value.
pub(super) struct CaptureTime {
    pub original: String,
    pub nanos: i64,
}

impl CaptureTime {
    /// Parses a timestamp with an explicit UTC offset.
    pub(super) fn parse(original: &str) -> Result<Self, String> {
        let original = original.trim();
        let nanos = DateTime::parse_from_rfc3339(original)
            .ok()
            .and_then(|time| time.timestamp_nanos_opt())
            .ok_or("请输入含时区的原始采集时间，例如 2026-10-09T08:00:00+08:00")?;
        Ok(Self {
            original: original.to_owned(),
            nanos,
        })
    }
}

impl Archive {
    /// Reads and validates a complete GOOD inventory without writing to SQLite.
    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|file| file.take(MAX_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|error| format!("无法读取 GOOD 文件：{error}"))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("GOOD 文件超过 16 MB".into());
        }
        let Object(good): Object<Good> =
            serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
                .map_err(|error| format!("GOOD 文件格式无效：{error}"))?;
        if good.format != "GOOD" || !matches!(good.version, 1..=3) || good.source.trim().is_empty()
        {
            return Err("请选择包含来源信息的 GOOD v1、v2 或 v3 文件".into());
        }
        if good.gi_off.iter().any(|category| {
            matches!(
                category.as_str(),
                "weapons" | "artifacts" | "materials" | "gi_player.numbers" | "gi_teapot"
            )
        }) {
            return Err("文件包含未完整共享的背包数据，请导出完整的武器、圣遗物和材料。".into());
        }
        let omitted_categories = [
            (good.weapons.is_none(), "武器"),
            (good.artifacts.is_none(), "圣遗物"),
            (good.materials.is_none(), "材料"),
        ]
        .into_iter()
        .filter_map(|(omitted, name)| omitted.then_some(name))
        .collect::<Vec<_>>();
        if omitted_categories.len() == 3
            || (!omitted_categories.is_empty()
                && (good.version != 2 || good.source != "Inventory_Kamera"))
        {
            return Err("完整背包快照须包含武器、圣遗物和材料。请检查导出范围。".into());
        }
        let (uid, captured_at) = good.identity()?;
        let inventory = Inventory {
            weapons: good
                .weapons
                .unwrap_or_default()
                .into_iter()
                .map(|Object(weapon)| weapon)
                .collect(),
            artifacts: good
                .artifacts
                .unwrap_or_default()
                .into_iter()
                .map(|Object(artifact)| artifact)
                .collect(),
            materials: good
                .materials
                .map_or_else(BTreeMap::new, |MaterialCounts(materials)| materials),
        };
        inventory.validate()?;
        Ok(Self {
            version: good.version,
            source: good.source,
            uid,
            captured_at,
            inventory,
            omitted_categories,
        })
    }

    /// Suggests the local time embedded by Inventory Kamera in its original filename.
    pub(super) fn suggested_time(&self, path: &Path) -> String {
        if let Some(time) = &self.captured_at {
            return time.clone();
        }
        if self.source != "Inventory_Kamera" {
            return String::new();
        }
        path.file_stem()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("genshinData_GOOD_"))
            .and_then(|time| NaiveDateTime::parse_from_str(time, "%Y_%m_%d_%H_%M").ok())
            .map_or_else(String::new, |time| {
                time.format("%Y-%m-%dT%H:%M:%S").to_string()
            })
    }
}

impl Good {
    fn identity(&self) -> Result<(Option<String>, Option<String>), String> {
        let (metadata_uid, metadata_time) = self
            .metadata
            .as_ref()
            .map_or((None, None), |Object(metadata)| {
                (metadata.uid.as_deref(), metadata.captured_at.as_deref())
            });
        let leyline_uid = self
            .gi_player
            .as_ref()
            .and_then(|Object(player)| player.uid)
            .map(|uid| uid.to_string());
        if let (Some(metadata_uid), Some(leyline_uid)) = (metadata_uid, &leyline_uid)
            && metadata_uid != leyline_uid
        {
            return Err("GOOD 文件中的游戏 UID 相互冲突".into());
        }
        let uid = metadata_uid.map(str::to_owned).or(leyline_uid);
        if let Some(uid) = &uid {
            validate_uid(uid).map_err(|_| "GOOD 文件中的游戏 UID 无效")?;
        }
        let metadata_time = metadata_time.map(CaptureTime::parse).transpose()?;
        let good_time = self
            .timestamp
            .map(|millis| {
                let timestamp = DateTime::from_timestamp_millis(millis)
                    .ok_or("GOOD 文件中的采集时间无效")?
                    .to_rfc3339_opts(SecondsFormat::Millis, true);
                CaptureTime::parse(&timestamp)
            })
            .transpose()?;
        if let (Some(metadata_time), Some(good_time)) = (&metadata_time, &good_time)
            && metadata_time.nanos != good_time.nanos
        {
            return Err("GOOD 文件中的采集时间相互冲突".into());
        }
        Ok((uid, metadata_time.or(good_time).map(|time| time.original)))
    }
}

impl Inventory {
    fn validate(&self) -> Result<(), String> {
        for (index, weapon) in self.weapons.iter().enumerate() {
            if weapon.key.trim().is_empty()
                || !(1..=100).contains(&weapon.level)
                || weapon.ascension > 6
                || !(1..=5).contains(&weapon.refinement)
            {
                return Err(format!("第 {} 件武器的记录无效", index + 1));
            }
        }
        for (index, artifact) in self.artifacts.iter().enumerate() {
            if artifact.set_key.trim().is_empty()
                || artifact.slot_key.trim().is_empty()
                || artifact.main_stat_key.trim().is_empty()
                || !(1..=5).contains(&artifact.rarity)
                || artifact.level > 20
                || artifact.substats.iter().any(|stat| {
                    stat.value < 0.0 || (stat.key.trim().is_empty() && stat.value != 0.0)
                })
            {
                return Err(format!("第 {} 件圣遗物的记录无效", index + 1));
            }
        }
        if self.materials.keys().any(|key| key.trim().is_empty()) {
            return Err("材料记录须包含有效的 GOOD 标识".into());
        }
        Ok(())
    }

    /// Encodes the previous inventory as a GOOD file that can be imported again.
    pub(super) fn encode(
        &self,
        version: u8,
        source: &str,
        uid: &str,
        captured_at: &str,
    ) -> Result<Vec<u8>, String> {
        #[derive(Serialize)]
        struct Export<'a> {
            format: &'static str,
            version: u8,
            source: &'a str,
            metadata: Metadata,
            weapons: &'a [Weapon],
            artifacts: &'a [Artifact],
            materials: &'a BTreeMap<String, u32>,
        }
        let bytes = serde_json::to_vec(&Export {
            format: "GOOD",
            version,
            source,
            metadata: Metadata {
                uid: Some(uid.to_owned()),
                captured_at: Some(captured_at.to_owned()),
            },
            weapons: &self.weapons,
            artifacts: &self.artifacts,
            materials: &self.materials,
        })
        .map_err(|error| format!("无法编码背包备份：{error}"))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("背包记录编码后超过 16 MB".into());
        }
        Ok(bytes)
    }
}

impl Weapon {
    /// Formats the weapon's level, ascension, refinement and equipment status.
    pub(super) fn label(&self) -> String {
        format!(
            "{} · 等级 {} · 突破 {} · 精炼 {}{}",
            self.key,
            self.level,
            self.ascension,
            self.refinement,
            equipment_status(self.location.as_deref(), self.lock),
        )
    }
}

impl Artifact {
    /// Formats the artifact's slot, progression, properties and equipment status.
    pub(super) fn label(&self) -> String {
        let slot = match self.slot_key.as_str() {
            "flower" => "生之花",
            "plume" => "死之羽",
            "sands" => "时之沙",
            "goblet" => "空之杯",
            "circlet" => "理之冠",
            other => other,
        };
        let mut label = format!(
            "{} · {slot} · {} 星 · +{} · 主词条 {}{}",
            self.set_key,
            self.rarity,
            self.level,
            self.main_stat_key,
            equipment_status(self.location.as_deref(), self.lock),
        );
        let stats = self
            .substats
            .iter()
            .filter(|stat| !stat.key.trim().is_empty() && stat.value > 0.0)
            .map(|stat| {
                let unit = if stat.key.ends_with('_') { "%" } else { "" };
                format!("{} {}{unit}", stat.key, stat.value)
            })
            .collect::<Vec<_>>()
            .join("、");
        if !stats.is_empty() {
            label.push_str(" · 副词条 ");
            label.push_str(&stats);
        }
        label
    }
}

fn equipment_status(location: Option<&str>, locked: Option<bool>) -> String {
    let mut status = String::new();
    if let Some(owner) = location.filter(|owner| !owner.is_empty()) {
        status.push_str(" · 装备于 ");
        status.push_str(owner);
    }
    if locked == Some(true) {
        status.push_str(" · 已锁定");
    }
    status
}
