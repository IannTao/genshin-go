use super::archive::{Archive, Record, encode};
use crate::account_store::{database_error, open};
use crate::native_environment::atomic_write;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Represents the destination and changes authorized by an import preview.
#[derive(PartialEq, Eq)]
pub(crate) struct Preview {
    /// Identifies the destination UID.
    pub(super) uid: String,
    before: Vec<Record>,
    changes: Vec<Record>,
    additions: usize,
    unchanged: usize,
    duplicates: usize,
    conflicts: usize,
}

impl Preview {
    /// Describes the scope and effects of the pending merge.
    pub(super) fn summary(&self) -> String {
        format!(
            "导入至 UID {}：新增 {}，更新 {}，无变化 {}",
            self.uid,
            self.additions,
            self.changes.len() - self.additions,
            self.unchanged
        )
    }

    /// Indicates whether committing the preview would change saved progress.
    pub(crate) fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }

    /// Reports matching and conflicting rows from the source file and saved progress.
    pub(super) fn overlap(&self) -> String {
        format!(
            "重复 {}，冲突 {}（冲突按状态与进度自动合并）",
            self.duplicates, self.conflicts
        )
    }

    /// Describes the effects of restoring achievement progress for this UID.
    pub(crate) fn restore_summary(&self) -> String {
        format!(
            "UID {} · 成就新增 {}，更新 {}，重复 {}，冲突 {}",
            self.uid,
            self.additions,
            self.changes.len() - self.additions,
            self.duplicates,
            self.conflicts
        )
    }
}

/// Prepares a merge for one existing UID without writing any achievement progress.
pub(super) fn preview(path: &Path, uid: &str, archive: &Archive) -> Result<Preview, String> {
    if archive.uid.as_deref().is_some_and(|source| source != uid) {
        return Err("必须导入文件声明的 UID 数据空间".into());
    }
    let db = open(path)?;
    Ok(plan(uid, archive, load_records(&db, uid)?))
}

/// Compares validated incoming progress with the saved records for the same UID.
pub(crate) fn plan(uid: &str, archive: &Archive, before: Vec<Record>) -> Preview {
    let existing: BTreeMap<_, _> = before.iter().map(|record| (record.id, *record)).collect();
    let mut result = Preview {
        uid: uid.to_owned(),
        before,
        changes: Vec::new(),
        additions: 0,
        unchanged: 0,
        duplicates: archive.duplicates,
        conflicts: archive.conflicts,
    };
    for (&id, &incoming) in &archive.records {
        if let Some(&saved) = existing.get(&id) {
            if incoming == saved {
                result.duplicates += 1;
            } else {
                result.conflicts += 1;
            }
            let merged = saved.merge(incoming);
            if merged == saved {
                result.unchanged += 1;
            } else {
                result.changes.push(merged);
            }
        } else {
            result.additions += 1;
            result.changes.push(incoming);
        }
    }
    result
}

/// Backs up the current progress and applies the confirmed merge in one SQLite transaction.
pub(super) fn import(path: &Path, preview: &Preview) -> Result<PathBuf, String> {
    if !preview.has_changes() {
        return Err("没有需要导入的成就变更".into());
    }
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let uid = &preview.uid;
    if load_records(&write, uid)? != preview.before {
        return Err("本机成就进度已变更，请重新预览".into());
    }
    let backup = write_archive(
        &path.with_file_name("achievement-backups"),
        uid,
        &preview.before,
    )
    .map_err(|error| format!("导入前备份失败：{error}"))?;
    apply(&write, preview)?;
    write.commit().map_err(|error| database_error(&error))?;
    Ok(backup)
}

/// Writes the planned achievement changes into the caller's transaction.
pub(crate) fn apply(db: &Connection, preview: &Preview) -> Result<(), String> {
    upsert_records(db, &preview.uid, &preview.changes)
}

/// Saves a manual completion change against the latest record for this UID.
pub(super) fn set_completion(
    path: &Path,
    uid: &str,
    id: u32,
    completed: bool,
) -> Result<(), String> {
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let saved = write
        .query_row(
            "SELECT record_json FROM achievement_records WHERE uid = ?1 AND achievement_id = ?2",
            params![uid, id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| database_error(&error))?;
    let record = saved
        .as_deref()
        .map_or_else(|| Ok(Record::unfinished(id)), decode_record)?;
    let updated = record.with_completion(completed);
    if record != updated {
        upsert_records(&write, uid, &[updated])?;
    }
    write.commit().map_err(|error| database_error(&error))
}

fn upsert_records(db: &Connection, uid: &str, records: &[Record]) -> Result<(), String> {
    let mut upsert = db
        .prepare(
            "INSERT INTO achievement_records (uid, achievement_id, record_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (uid, achievement_id)
             DO UPDATE SET record_json = excluded.record_json",
        )
        .map_err(|error| database_error(&error))?;
    for record in records {
        let json = serde_json::to_string(record).map_err(|error| error.to_string())?;
        upsert
            .execute(params![uid, record.id, json])
            .map_err(|error| database_error(&error))?;
    }
    Ok(())
}

fn decode_record(json: &str) -> Result<Record, String> {
    serde_json::from_str(json).map_err(|error| format!("读取成就进度失败：{error}"))
}

/// Reads all stored progress for a UID from the caller's database view.
pub(crate) fn load_records(db: &Connection, uid: &str) -> Result<Vec<Record>, String> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM uid_history WHERE uid = ?1)",
            [uid],
            |row| row.get(0),
        )
        .map_err(|error| database_error(&error))?;
    if !exists {
        return Err("该 UID 数据空间不存在，请先在角色资料中创建".into());
    }
    let mut select = db
        .prepare(
            "SELECT record_json FROM achievement_records
             WHERE uid = ?1 ORDER BY achievement_id",
        )
        .map_err(|error| database_error(&error))?;
    select
        .query_map([uid], |row| row.get::<_, String>(0))
        .map_err(|error| database_error(&error))?
        .map(|json| {
            let json = json.map_err(|error| database_error(&error))?;
            decode_record(&json)
        })
        .collect()
}

/// Loads the saved achievement progress for one UID.
pub(super) fn records(path: &Path, uid: &str) -> Result<Vec<Record>, String> {
    load_records(&open(path)?, uid)
}

/// Exports the selected UID's achievement progress to a fresh UIAF file.
pub(super) fn export(path: &Path, uid: &str, folder: &Path) -> Result<PathBuf, String> {
    write_archive(folder, uid, &records(path, uid)?)
}

fn write_archive(folder: &Path, uid: &str, records: &[Record]) -> Result<PathBuf, String> {
    let unique = windows::core::GUID::new().map_err(|_| "无法生成成就档案文件名")?;
    let file = folder.join(format!("UIAF-{uid}-{unique:?}.json"));
    atomic_write(&file, &encode(uid, records)?)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_store::{delete_uid_history, select_uid};
    use serde_json::{Value, json};
    use std::fs;

    const UID: &str = "123450018";
    const OTHER_UID: &str = "123450019";
    const FIXTURE: &str = include_str!("../../tests/data/uiaf-v1.1.json");

    fn test_directory() -> Result<PathBuf, std::io::Error> {
        let directory = std::env::temp_dir().join(format!(
            "genshin-go-achievements-{:?}",
            windows::core::GUID::new().map_err(std::io::Error::other)?
        ));
        fs::create_dir(&directory)?;
        Ok(directory)
    }

    #[test]
    fn backs_up_then_merges_and_round_trips_uiaf_progress() -> Result<(), Box<dyn std::error::Error>>
    {
        let directory = test_directory()?;
        let path = directory.join("launcher.sqlite3");
        select_uid(&path, UID, true)?;
        select_uid(&path, OTHER_UID, true)?;
        let file = directory.join("input.json");
        fs::write(&file, FIXTURE)?;
        let original = Archive::read(&file)?;
        import(&path, &preview(&path, UID, &original)?)?;
        import(&path, &preview(&path, OTHER_UID, &original)?)?;

        let mut incoming: Value = serde_json::from_str(FIXTURE)?;
        incoming["info"]["uid"] = json!(123_450_018);
        incoming["list"] = json!([
            { "id": 80001.0, "current": 2.0, "status": 1.0, "timestamp": 0.0 },
            { "id": 80001, "current": 2, "status": 1, "timestamp": 0 },
            { "id": 80002, "current": 0, "status": 3, "timestamp": 253_402_271_999_i64 },
            { "id": 80002, "current": 40, "status": 2, "timestamp": 1_600_963_300 },
            { "id": 80003, "current": 0, "status": 0 }
        ]);
        fs::write(&file, serde_json::to_vec(&incoming)?)?;
        let incoming = Archive::read(&file)?;
        assert!(preview(&path, OTHER_UID, &incoming).is_err());
        let merge = preview(&path, UID, &incoming)?;
        assert_eq!(
            merge.summary(),
            "导入至 UID 123450018：新增 1，更新 2，无变化 0"
        );
        assert_eq!(
            merge.overlap(),
            "重复 1，冲突 3（冲突按状态与进度自动合并）"
        );

        let backup = import(&path, &merge)?;
        let saved_before = Archive::read(&backup)?;
        assert_eq!(saved_before.uid.as_deref(), Some(UID));
        assert_eq!(saved_before.records, original.records);
        assert!(!preview(&path, UID, &incoming)?.has_changes());
        let export_path = export(&path, UID, &directory)?;
        let exported: Value = serde_json::from_slice(&fs::read(&export_path)?)?;
        assert_eq!(exported["info"]["uiaf_version"], "v1.1");
        assert_eq!(
            exported["list"],
            json!([
                { "id": 80001, "current": 2, "status": 3, "timestamp": 1_600_963_200 },
                { "id": 80002, "current": 40, "status": 3, "timestamp": 1_600_963_300 },
                { "id": 80003, "current": 0, "status": 0, "timestamp": 253_402_271_999_i64 }
            ])
        );
        let saved = records(&path, UID)?;
        delete_uid_history(&path, UID)?;
        assert_eq!(records(&path, UID)?, []);
        assert_eq!(
            records(&path, OTHER_UID)?,
            original.records.values().copied().collect::<Vec<_>>()
        );
        let archive = Archive::read(&export_path)?;
        import(&path, &preview(&path, UID, &archive)?)?;
        assert_eq!(records(&path, UID)?, saved);
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn refuses_stale_previews_and_keeps_progress_when_backup_fails()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = test_directory()?;
        let path = directory.join("launcher.sqlite3");
        select_uid(&path, UID, true)?;
        let file = directory.join("input.json");
        fs::write(&file, FIXTURE)?;
        let archive = Archive::read(&file)?;
        let initial = preview(&path, UID, &archive)?;
        let backup_folder = directory.join("achievement-backups");
        fs::write(&backup_folder, "blocks the backup directory")?;
        let error = import(&path, &initial).err().ok_or("backup must fail")?;
        assert!(error.contains("导入前备份失败"));
        assert_eq!(records(&path, UID)?, []);

        fs::remove_file(&backup_folder)?;
        import(&path, &initial)?;
        let error = import(&path, &initial)
            .err()
            .ok_or("stale preview must fail")?;
        assert!(error.contains("请重新预览"));
        assert!(!preview(&path, UID, &archive)?.has_changes());
        fs::remove_dir_all(directory)?;
        Ok(())
    }
}
