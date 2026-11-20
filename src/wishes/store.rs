use super::archive::{Group, Pool, Wish, encode};
use super::statistics::{self, PoolStatistics};
use crate::account_store::{database_error, open};
use crate::native_environment::atomic_write;
use rusqlite::{Connection, TransactionBehavior, params};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Limits the number of wish rows rendered on a page.
pub(super) const PAGE_SIZE: i64 = 30;

/// Records the target, additions and skipped records approved in an import preview.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ImportPlan {
    /// Identifies the destination UID.
    pub(super) uid: String,
    /// Holds the validated records ready to insert.
    pub(super) additions: Vec<Wish>,
    duplicates: usize,
    conflicts: usize,
}

impl ImportPlan {
    /// Formats the target and import counts for confirmation.
    pub(super) fn summary(&self) -> String {
        format!(
            "导入至 UID {}：新增 {}，重复 {}，冲突 {}",
            self.uid,
            self.additions.len(),
            self.duplicates,
            self.conflicts
        )
    }

    /// Indicates whether any incoming records conflict and will be skipped.
    pub(super) fn has_conflicts(&self) -> bool {
        self.conflicts > 0
    }
}

/// Classifies the selected source group against a consistent view of the target UID.
pub(super) fn preview(path: &Path, uid: &str, group: &Group) -> Result<ImportPlan, String> {
    let mut db = open(path)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    check_target(&read, uid, group)?;
    Ok(plan(uid, group, &all(&read, uid)?))
}

/// Rechecks the preview, backs up the target history, and inserts all additions atomically.
pub(super) fn import(path: &Path, group: &Group, preview: &ImportPlan) -> Result<PathBuf, String> {
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let uid = &preview.uid;
    check_target(&write, uid, group)?;
    let before = all(&write, uid)?;
    let actual = plan(uid, group, &before);
    if actual != *preview {
        return Err("本机祈愿记录已变更，请重新预览".into());
    }
    if actual.additions.is_empty() {
        return Err("没有可导入的新增记录".into());
    }
    let backup = write_archive(&path.with_file_name("wish-backups"), uid, &before)
        .map_err(|error| format!("导入前备份失败：{error}"))?;
    {
        let mut insert = write
            .prepare(
                "INSERT INTO wish_records (uid, record_id, drawn_at, record_json)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .map_err(|error| database_error(&error))?;
        for wish in actual.additions {
            let json = serde_json::to_string(&wish).map_err(|error| error.to_string())?;
            insert
                .execute(params![uid, wish.id, wish.time, json])
                .map_err(|error| database_error(&error))?;
        }
    }
    write.commit().map_err(|error| database_error(&error))?;
    Ok(backup)
}

fn check_target(db: &Connection, uid: &str, group: &Group) -> Result<(), String> {
    if group.uid.as_deref().is_some_and(|source| source != uid) {
        return Err("必须导入文件所声明的 UID 数据空间".into());
    }
    let found: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM uid_history WHERE uid = ?1)",
            [uid],
            |row| row.get(0),
        )
        .map_err(|error| database_error(&error))?;
    if !found {
        return Err("该 UID 数据空间不存在，请先创建".into());
    }
    Ok(())
}

fn plan(uid: &str, group: &Group, before: &[Wish]) -> ImportPlan {
    let existing: BTreeMap<&str, &Wish> =
        before.iter().map(|wish| (wish.id.as_str(), wish)).collect();
    let mut incoming = BTreeMap::<&str, Vec<&Wish>>::new();
    for wish in &group.list {
        incoming.entry(&wish.id).or_default().push(wish);
    }
    let mut result = ImportPlan {
        uid: uid.to_owned(),
        additions: Vec::new(),
        duplicates: 0,
        conflicts: 0,
    };
    for (id, entries) in incoming {
        if let Some(saved) = existing.get(id) {
            let matching: Vec<_> = entries
                .iter()
                .copied()
                .filter(|wish| saved.agrees_with(wish))
                .collect();
            let duplicates = if merge(saved, matching.iter().copied()).is_some() {
                matching.len()
            } else {
                0
            };
            result.duplicates += duplicates;
            result.conflicts += entries.len() - duplicates;
        } else if let Some((first, rest)) = entries.split_first() {
            if let Some(merged) = merge(first, rest.iter().copied()) {
                result.additions.push(merged);
                result.duplicates += rest.len();
            } else {
                result.conflicts += entries.len();
            }
        }
    }
    result
}

fn merge<'a>(base: &Wish, entries: impl Iterator<Item = &'a Wish>) -> Option<Wish> {
    let mut merged = base.clone();
    for wish in entries {
        if !merged.agrees_with(wish) {
            return None;
        }
        merged.fill_missing_fields(wish);
    }
    Some(merged)
}

/// Loads the total and visible page of wish records for one UID.
pub(super) fn page(path: &Path, uid: &str, index: i64) -> Result<(i64, Vec<Wish>), String> {
    let mut db = open(path)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    let total = read
        .query_row(
            "SELECT count(*) FROM wish_records WHERE uid = ?1",
            [uid],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| database_error(&error))?;
    let index = index.min((total - 1).max(0) / PAGE_SIZE);
    Ok((total, records(&read, uid, PAGE_SIZE, index * PAGE_SIZE)?))
}

fn all(db: &Connection, uid: &str) -> Result<Vec<Wish>, String> {
    records(db, uid, -1, 0)
}

/// Reads and summarizes every saved draw for the selected UID.
pub(super) fn statistics(path: &Path, uid: &str) -> Result<BTreeMap<Pool, PoolStatistics>, String> {
    statistics::summarize(&all(&open(path)?, uid)?)
}

fn records(db: &Connection, uid: &str, limit: i64, offset: i64) -> Result<Vec<Wish>, String> {
    let mut select = db
        .prepare(
            "SELECT record_json FROM wish_records WHERE uid = ?1
         ORDER BY drawn_at DESC, length(record_id) DESC, record_id DESC LIMIT ?2 OFFSET ?3",
        )
        .map_err(|error| database_error(&error))?;
    select
        .query_map(params![uid, limit, offset], |row| row.get::<_, String>(0))
        .map_err(|error| database_error(&error))?
        .map(|json| {
            let json = json.map_err(|error| database_error(&error))?;
            serde_json::from_str(&json).map_err(|error| format!("读取保存的祈愿记录失败：{error}"))
        })
        .collect()
}

/// Writes the selected UID's complete wish history to a new UIGF archive.
pub(super) fn export(path: &Path, uid: &str, folder: &Path) -> Result<PathBuf, String> {
    let list = all(&open(path)?, uid)?;
    write_archive(folder, uid, &list)
}

fn write_archive(folder: &Path, uid: &str, list: &[Wish]) -> Result<PathBuf, String> {
    let unique = windows::core::GUID::new().map_err(|_| "无法生成祈愿档案文件名")?;
    let file = folder.join(format!("UIGF-{uid}-{unique:?}.json"));
    atomic_write(&file, &encode(uid, list)?)?;
    Ok(file)
}
