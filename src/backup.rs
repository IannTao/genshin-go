//! Portable backups of the Windows launcher's UID data.
#[cfg(test)]
mod tests;
mod ui;

use crate::account_store::{
    database_error, ensure_active_uid, ensure_uid_space, list_uids, open, snapshot,
};
use crate::achievements::{archive::Archive, archive::Record, store as achievements};
use crate::archive_json::{Object, object_list};
use crate::growth::backup as growth;
use crate::inventory::backup as inventory;
use crate::native_environment::atomic_write;
use crate::profile::{Profile, validate_uid};
use crate::wishes::{archive::Wish, store as wishes};
use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Renders the overall backup and restore controls on the UID spaces page.
pub(crate) fn section(
    path: Result<PathBuf, String>,
    profile: nsh_core::prelude::Signal<Profile>,
    revision: nsh_core::prelude::Signal<u64>,
    growth_revision: nsh_core::prelude::Signal<u64>,
) -> nsh_core::prelude::View {
    ui::section(path, profile, revision, growth_revision)
}

const VERSION: u32 = 2;

#[derive(Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Format {
    GenshinGoBackup,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    format: Format,
    version: u32,
    #[serde(deserialize_with = "object_list")]
    uids: Vec<UidBackup>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UidBackup {
    uid: String,
    #[serde(deserialize_with = "object_list")]
    wishes: Vec<Wish>,
    #[serde(deserialize_with = "object_list")]
    achievements: Vec<Record>,
    #[serde(default)]
    growth: growth::Data,
    #[serde(default)]
    inventory: inventory::Data,
}

impl Backup {
    fn read(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|error| format!("无法读取整体备份：{error}"))?;
        let Object(mut backup): Object<Self> =
            serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
                .map_err(|error| format!("整体备份格式无效：{error}"))?;
        if !matches!(backup.version, 1 | VERSION) {
            return Err(format!("不支持整体备份版本 {}", backup.version));
        }
        let mut uids = BTreeSet::new();
        for entry in &mut backup.uids {
            validate_uid(&entry.uid).map_err(|_| format!("整体备份中的 UID {} 无效", entry.uid))?;
            if !uids.insert(entry.uid.clone()) {
                return Err(format!("整体备份中的 UID {} 重复", entry.uid));
            }
            if backup.version == 1 && !(entry.growth.is_empty() && entry.inventory.is_empty()) {
                return Err("第 1 版整体备份仅支持祈愿记录和成就进度".into());
            }
            entry
                .growth
                .validate()
                .and_then(|()| entry.inventory.validate(&entry.uid))
                .map_err(|error| format!("UID {}：{error}", entry.uid))?;
            for (index, wish) in entry.wishes.iter_mut().enumerate() {
                wish.normalize(0).map_err(|error| {
                    format!(
                        "UID {} 的第 {} 条祈愿记录无效：{error}",
                        entry.uid,
                        index + 1
                    )
                })?;
            }
        }
        backup.uids.sort_by(|left, right| left.uid.cmp(&right.uid));
        Ok(backup)
    }

    fn capture(db: &Connection) -> Result<Self, String> {
        let uids = list_uids(db)
            .map_err(|error| database_error(&error))?
            .into_iter()
            .map(|uid| {
                Ok(UidBackup {
                    wishes: wishes::all(db, &uid)?,
                    achievements: achievements::load_records(db, &uid)?,
                    growth: growth::Data::capture(db, &uid)?,
                    inventory: inventory::Data::capture(db, &uid)?,
                    uid,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            format: Format::GenshinGoBackup,
            version: VERSION,
            uids,
        })
    }

    fn write(&self, folder: &Path) -> Result<PathBuf, String> {
        let unique = windows::core::GUID::new().map_err(|_| "无法生成整体备份文件名")?;
        let file = folder.join(format!("Genshin-Go-{unique:?}.json"));
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        atomic_write(&file, &bytes)?;
        Ok(file)
    }
}

fn export(path: &Path) -> Result<PathBuf, String> {
    let mut db = open(path)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    let backup = Backup::capture(&read)?;
    read.commit().map_err(|error| database_error(&error))?;
    backup.write(&path.with_file_name("uid-backups"))
}

#[derive(PartialEq, Eq)]
struct UidRestore {
    uid: String,
    create: bool,
    wishes: wishes::ImportPlan,
    achievements: achievements::Preview,
    growth: growth::Restore,
    inventory: inventory::Restore,
}

impl UidRestore {
    fn has_changes(&self, choices: Option<&growth::Choices>) -> bool {
        self.create
            || self.wishes.has_changes()
            || self.achievements.has_changes()
            || self.growth.has_changes(choices)
            || self.inventory.has_additions()
    }

    fn summaries(&self) -> impl Iterator<Item = String> + '_ {
        [
            self.wishes.restore_summary(),
            self.achievements.restore_summary(),
        ]
        .into_iter()
        .chain(self.growth.summaries(&self.uid))
        .chain(self.inventory.summaries(&self.uid))
    }
}

struct RestorePreview {
    source: Backup,
    entries: Vec<UidRestore>,
}

type Choices = BTreeMap<String, growth::Choices>;

impl RestorePreview {
    fn choices_complete(&self, choices: &Choices) -> bool {
        self.entries
            .iter()
            .all(|entry| entry.growth.choices_complete(choices.get(&entry.uid)))
    }

    fn has_changes(&self, choices: &Choices) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.has_changes(choices.get(&entry.uid)))
    }

    fn changes_growth_for(&self, uid: &str, choices: &Choices) -> bool {
        self.entries
            .iter()
            .find(|entry| entry.uid == uid)
            .is_some_and(|entry| entry.growth.has_changes(choices.get(uid)))
    }
}

fn plan(source: &Backup, local: &Backup) -> Vec<UidRestore> {
    let existing: BTreeMap<_, _> = local.uids.iter().map(|entry| (&entry.uid, entry)).collect();
    let empty_growth = growth::Data::default();
    let empty_inventory = inventory::Data::default();
    source
        .uids
        .iter()
        .map(|entry| {
            let before = existing.get(&entry.uid);
            let progress = Archive::from_records(None, entry.achievements.iter().copied());
            UidRestore {
                uid: entry.uid.clone(),
                create: before.is_none(),
                wishes: wishes::plan(
                    &entry.uid,
                    &entry.wishes,
                    before.map_or(&[], |saved| saved.wishes.as_slice()),
                ),
                achievements: achievements::plan(
                    &entry.uid,
                    &progress,
                    before.map_or_else(Vec::new, |saved| saved.achievements.clone()),
                ),
                growth: entry
                    .growth
                    .plan(before.map_or(&empty_growth, |saved| &saved.growth)),
                inventory: entry
                    .inventory
                    .plan(before.map_or(&empty_inventory, |saved| &saved.inventory)),
            }
        })
        .collect()
}

fn preview(database: &Path, file: &Path) -> Result<RestorePreview, String> {
    let source = Backup::read(file)?;
    let mut db = open(database)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    let entries = plan(&source, &Backup::capture(&read)?);
    Ok(RestorePreview { source, entries })
}

fn restore(
    path: &Path,
    preview: &RestorePreview,
    choices: &Choices,
) -> Result<(PathBuf, Profile), String> {
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let before = Backup::capture(&write)?;
    let entries = plan(&preview.source, &before);
    if entries != preview.entries {
        return Err("本机 UID 记录已变更，请重新预览".into());
    }
    if !preview.choices_complete(choices) {
        return Err("请先为每项冲突选择要保留的版本".into());
    }
    if !preview.has_changes(choices) {
        return Err("没有需要恢复的变更".into());
    }
    let backup = before
        .write(&path.with_file_name("uid-backups"))
        .map_err(|error| format!("恢复前备份失败：{error}"))?;
    for (entry, source) in entries.iter().zip(&preview.source.uids) {
        let selected = choices.get(&entry.uid);
        if !entry.has_changes(selected) {
            continue;
        }
        if entry.create {
            ensure_uid_space(&write, &entry.uid).map_err(|error| database_error(&error))?;
        }
        wishes::insert(&write, &entry.wishes)?;
        achievements::apply(&write, &entry.achievements)?;
        entry.growth.apply(&write, &entry.uid, selected)?;
        source
            .inventory
            .apply(&write, &entry.uid, &entry.inventory)?;
    }
    if let Some(first) = entries.first() {
        ensure_active_uid(&write, &first.uid).map_err(|error| database_error(&error))?;
    }
    let profile = snapshot(&write).map_err(|error| database_error(&error))?;
    write.commit().map_err(|error| database_error(&error))?;
    Ok((backup, profile))
}
