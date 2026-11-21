//! Portable backups of the Windows launcher's UID data.
#[cfg(test)]
mod tests;
mod ui;

use crate::account_store::{
    database_error, ensure_active_uid, ensure_uid_space, list_uids, open, snapshot,
};
use crate::achievements::{archive::Archive, archive::Record, store as achievements};
use crate::archive_json::Object;
use crate::native_environment::atomic_write;
use crate::profile::{Profile, validate_uid};
use crate::wishes::{archive::Wish, store as wishes};
use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Renders the overall backup and restore controls on the UID spaces page.
pub(crate) fn section(
    path: Result<PathBuf, String>,
    profile: nsh_core::prelude::Signal<Profile>,
    revision: nsh_core::prelude::Signal<u64>,
) -> nsh_core::prelude::View {
    ui::section(path, profile, revision)
}

const VERSION: u32 = 1;

#[derive(Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Format {
    GenshinGoBackup,
}

#[derive(Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    format: Format,
    version: u32,
    #[serde(deserialize_with = "object_list")]
    uids: Vec<UidBackup>,
}

#[derive(Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct UidBackup {
    uid: String,
    #[serde(deserialize_with = "object_list")]
    wishes: Vec<Wish>,
    #[serde(deserialize_with = "object_list")]
    achievements: Vec<Record>,
}

fn object_list<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Vec<T>, D::Error> {
    Ok(Vec::<Object<T>>::deserialize(decoder)?
        .into_iter()
        .map(|Object(value)| value)
        .collect())
}

impl Backup {
    fn read(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|error| format!("无法读取整体备份：{error}"))?;
        let Object(mut backup): Object<Self> =
            serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
                .map_err(|error| format!("整体备份格式无效：{error}"))?;
        if backup.version != VERSION {
            return Err(format!("不支持整体备份版本 {}", backup.version));
        }
        let mut uids = BTreeSet::new();
        for entry in &mut backup.uids {
            validate_uid(&entry.uid).map_err(|_| format!("整体备份中的 UID {} 无效", entry.uid))?;
            if !uids.insert(entry.uid.clone()) {
                return Err(format!("整体备份中的 UID {} 重复", entry.uid));
            }
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
}

impl UidRestore {
    fn has_changes(&self) -> bool {
        self.create || self.wishes.has_changes() || self.achievements.has_changes()
    }

    fn summaries(&self) -> [String; 2] {
        [
            self.wishes.restore_summary(),
            self.achievements.restore_summary(),
        ]
    }
}

#[derive(Eq, PartialEq)]
struct RestorePreview {
    source: Backup,
    entries: Vec<UidRestore>,
}

impl RestorePreview {
    fn has_changes(&self) -> bool {
        self.entries.iter().any(UidRestore::has_changes)
    }
}

fn plan(source: &Backup, local: &Backup) -> Vec<UidRestore> {
    let existing: BTreeMap<_, _> = local.uids.iter().map(|entry| (&entry.uid, entry)).collect();
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

fn restore(path: &Path, preview: &RestorePreview) -> Result<(PathBuf, Profile), String> {
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let before = Backup::capture(&write)?;
    let entries = plan(&preview.source, &before);
    if entries != preview.entries {
        return Err("本机 UID 记录已变更，请重新预览".into());
    }
    if !preview.has_changes() {
        return Err("没有需要恢复的变更".into());
    }
    let backup = before
        .write(&path.with_file_name("uid-backups"))
        .map_err(|error| format!("恢复前备份失败：{error}"))?;
    for entry in entries.iter().filter(|entry| entry.has_changes()) {
        if entry.create {
            ensure_uid_space(&write, &entry.uid).map_err(|error| database_error(&error))?;
        }
        wishes::insert(&write, &entry.wishes)?;
        achievements::apply(&write, &entry.achievements)?;
    }
    if let Some(first) = entries.first() {
        ensure_active_uid(&write, &first.uid).map_err(|error| database_error(&error))?;
    }
    let profile = snapshot(&write).map_err(|error| database_error(&error))?;
    write.commit().map_err(|error| database_error(&error))?;
    Ok((backup, profile))
}
