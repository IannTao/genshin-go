use super::{
    MaterialSnapshot,
    archive::{Archive, CaptureTime, Inventory},
};
use crate::{
    account_store::{database_error, open},
    native_environment::atomic_write,
    profile::VerifiedAccount,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Creates inventory history and a single catalog of GOOD material keys.
pub(super) const SCHEMA: &str = "
    CREATE TABLE inventory_snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        uid TEXT NOT NULL REFERENCES uid_history(uid) ON DELETE CASCADE,
        captured_at TEXT NOT NULL,
        captured_nanos INTEGER NOT NULL,
        source TEXT NOT NULL,
        good_version INTEGER NOT NULL,
        weapons_json TEXT NOT NULL,
        artifacts_json TEXT NOT NULL,
        UNIQUE (uid, captured_nanos)
    );
    CREATE TABLE material_catalog (good_key TEXT NOT NULL PRIMARY KEY);
    CREATE TABLE inventory_materials (
        snapshot_id INTEGER NOT NULL REFERENCES inventory_snapshots(id) ON DELETE CASCADE,
        good_key TEXT NOT NULL REFERENCES material_catalog(good_key),
        quantity INTEGER NOT NULL CHECK (quantity >= 0),
        PRIMARY KEY (snapshot_id, good_key)
    );";

/// Describes the saved snapshot and its original collection time.
#[derive(PartialEq, Eq)]
pub(super) struct Header {
    pub id: i64,
    pub captured_at: String,
    pub source: String,
    pub version: u8,
    /// Identifies the saved categories.
    pub coverage: u8,
    /// Contains the time used to sort snapshots across time zones.
    pub(super) captured_nanos: i64,
}

/// Describes the visible effect of confirming a reviewed import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ImportEffect {
    /// Makes the incoming snapshot the default inventory.
    Latest,
    /// Saves the incoming snapshot for explicit historical browsing.
    Historical,
    /// Leaves the existing history unchanged.
    Duplicate,
}

/// Identifies the import destination and evidence used to recheck confirmation.
#[derive(PartialEq, Eq)]
pub(super) struct Preview {
    pub uid: String,
    pub effect: ImportEffect,
    pub coverage: u8,
    pub replaced: Vec<Header>,
    last_inserted: Option<i64>,
}

/// Lists a UID's snapshots in descending order of collection time.
pub(super) fn list(path: &Path, uid: &str) -> Result<Vec<Header>, String> {
    headers(&open(path)?, uid)
}

/// Lists snapshot headers from the caller's database snapshot.
pub(super) fn headers(db: &Connection, uid: &str) -> Result<Vec<Header>, String> {
    let mut query = db
        .prepare(
            "SELECT id, captured_at, source, good_version, captured_nanos, coverage
             FROM inventory_snapshots WHERE uid = ?1 ORDER BY captured_nanos DESC",
        )
        .map_err(|error| database_error(&error))?;
    query
        .query_map([uid], |row| {
            Ok(Header {
                id: row.get(0)?,
                captured_at: row.get(1)?,
                source: row.get(2)?,
                version: row.get(3)?,
                captured_nanos: row.get(4)?,
                coverage: row.get(5)?,
            })
        })
        .and_then(Iterator::collect)
        .map_err(|error| database_error(&error))
}

/// Loads equipment and materials for one snapshot from a consistent database view.
pub(super) fn read(path: &Path, uid: &str, id: i64) -> Result<Inventory, String> {
    let mut db = open(path)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    inventory(&read, uid, id)
}

/// Reads the latest snapshot's material quantities and collection time in one transaction.
pub(super) fn latest_materials(path: &Path, uid: &str) -> Result<Option<MaterialSnapshot>, String> {
    let mut db = open(path)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    let snapshot = read
        .query_row(
            "SELECT id, captured_at FROM inventory_snapshots
             WHERE uid = ?1 AND (coverage & ?2) <> 0 ORDER BY captured_nanos DESC LIMIT 1",
            params![uid, super::MATERIALS],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| database_error(&error))?;
    snapshot
        .map(|(id, captured_at)| {
            Ok(MaterialSnapshot {
                captured_at,
                quantities: materials(&read, id)?,
            })
        })
        .transpose()
}

/// Reads a saved inventory from the caller's database snapshot.
pub(super) fn inventory(db: &Connection, uid: &str, id: i64) -> Result<Inventory, String> {
    let (weapons, artifacts) = db
        .query_row(
            "SELECT weapons_json, artifacts_json FROM inventory_snapshots WHERE uid = ?1 AND id = ?2",
            params![uid, id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|error| database_error(&error))?;
    let decode_error = |error| format!("无法读取已保存的背包记录：{error}");
    Ok(Inventory {
        weapons: serde_json::from_str(&weapons).map_err(decode_error)?,
        artifacts: serde_json::from_str(&artifacts).map_err(decode_error)?,
        materials: materials(db, id)?,
    })
}

fn materials(db: &Connection, id: i64) -> Result<BTreeMap<String, u32>, String> {
    let mut query = db
        .prepare("SELECT good_key, quantity FROM inventory_materials WHERE snapshot_id = ?1")
        .map_err(|error| database_error(&error))?;
    query
        .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))
        .and_then(Iterator::collect)
        .map_err(|error| database_error(&error))
}

/// Reads the current inventory and determines how the incoming snapshot would be stored.
pub(super) fn preview(
    path: &Path,
    uid: &str,
    archive: &Archive,
    captured_at: &CaptureTime,
    coverage: u8,
) -> Result<Preview, String> {
    let mut db = open(path)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    plan(&read, uid, archive, captured_at, coverage)
}

fn plan(
    db: &Connection,
    uid: &str,
    archive: &Archive,
    captured_at: &CaptureTime,
    coverage: u8,
) -> Result<Preview, String> {
    if archive
        .uid
        .as_deref()
        .is_some_and(|declared| declared != uid)
    {
        return Err("请使用文件所声明的 UID 数据空间".into());
    }
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM uid_history WHERE uid = ?1)",
            [uid],
            |row| row.get(0),
        )
        .map_err(|error| database_error(&error))?;
    if !exists {
        return Err("请先创建导入目标的 UID 数据空间".into());
    }
    let history = headers(db, uid)?;
    let last_inserted = history.iter().map(|header| header.id).max();
    let effect = if let Some(existing) = history
        .iter()
        .find(|entry| entry.captured_nanos == captured_at.nanos)
    {
        if existing.coverage != coverage || inventory(db, uid, existing.id)? != archive.inventory {
            return Err("已有采集时间相同但内容不同的快照，请核对原始采集时间".into());
        }
        ImportEffect::Duplicate
    } else if history
        .iter()
        .take_while(|header| header.captured_nanos > captured_at.nanos)
        .fold(0, |newer, header| newer | header.coverage)
        & coverage
        == coverage
    {
        ImportEffect::Historical
    } else {
        ImportEffect::Latest
    };
    let mut remaining = coverage;
    let replaced = history
        .into_iter()
        .filter(|header| {
            let affected = header.coverage & remaining;
            remaining &= !affected;
            affected != 0
        })
        .collect();
    Ok(Preview {
        uid: uid.to_owned(),
        effect,
        coverage,
        last_inserted,
        replaced,
    })
}

/// Rechecks the preview, backs up the affected categories and saves the snapshot.
pub(super) fn import(
    path: &Path,
    archive: &Archive,
    captured_at: &CaptureTime,
    preview: &Preview,
) -> Result<Vec<PathBuf>, String> {
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let actual = plan(&write, &preview.uid, archive, captured_at, preview.coverage)?;
    if &actual != preview {
        return Err("背包快照已变化，请重新预览".into());
    }
    let backups = commit_snapshot(
        path,
        &write,
        archive,
        captured_at,
        &actual,
        "导入前备份失败",
    )?;
    write.commit().map_err(|error| database_error(&error))?;
    Ok(backups)
}

/// Backs up the latest material snapshot and saves materials synchronized from `MiYouShe`.
pub(super) fn save_synced_materials(
    path: &Path,
    account: &VerifiedAccount,
    captured_at: &CaptureTime,
    source: &str,
    materials: BTreeMap<String, u32>,
) -> Result<(ImportEffect, Vec<PathBuf>), String> {
    if materials.is_empty() {
        return Err("米游社未返回有效的培养材料数量".into());
    }
    let inventory = Inventory {
        weapons: Vec::new(),
        artifacts: Vec::new(),
        materials,
    };
    inventory.validate()?;
    let archive = Archive {
        version: 3,
        source: source.to_owned(),
        uid: Some(account.uid.clone()),
        captured_at: Some(captured_at.original.clone()),
        inventory,
        coverage: super::MATERIALS,
        omitted_categories: Vec::new(),
    };
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let authorized: bool = write
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM account_authorization
                WHERE singleton = 1 AND account_id = ?1 AND uid = ?2 AND revision = ?3
             )",
            params![account.account_id, account.uid, account.revision],
            |row| row.get(0),
        )
        .map_err(|error| database_error(&error))?;
    if !authorized {
        return Err("米游社授权已变化或移除，本次材料同步结果未写入".into());
    }
    let actual = plan(
        &write,
        &account.uid,
        &archive,
        captured_at,
        super::MATERIALS,
    )?;
    let backups = commit_snapshot(
        path,
        &write,
        &archive,
        captured_at,
        &actual,
        "同步前备份失败",
    )?;
    write.commit().map_err(|error| database_error(&error))?;
    Ok((actual.effect, backups))
}

fn commit_snapshot(
    path: &Path,
    write: &Connection,
    archive: &Archive,
    captured_at: &CaptureTime,
    actual: &Preview,
    backup_error_prefix: &str,
) -> Result<Vec<PathBuf>, String> {
    if actual.effect == ImportEffect::Duplicate {
        return Err("这份背包快照已保存".into());
    }
    // Ensure the new snapshot fits the same file limit when backed up later.
    archive.inventory.encode(
        archive.version,
        &archive.source,
        &actual.uid,
        &captured_at.original,
        actual.coverage,
    )?;
    let backups =
        backup(path, write, actual).map_err(|error| format!("{backup_error_prefix}：{error}"))?;
    insert(
        write,
        &actual.uid,
        captured_at,
        archive.version,
        &archive.source,
        &archive.inventory,
        actual.coverage,
    )?;
    Ok(backups)
}

/// Saves the supplied categories as a snapshot in the caller's transaction.
pub(super) fn insert(
    db: &Connection,
    uid: &str,
    captured_at: &CaptureTime,
    version: u8,
    source: &str,
    inventory: &Inventory,
    coverage: u8,
) -> Result<(), String> {
    let encode_error = |error| format!("无法编码背包记录：{error}");
    let weapons = if coverage & super::WEAPONS != 0 {
        serde_json::to_string(&inventory.weapons).map_err(encode_error)?
    } else {
        "[]".into()
    };
    let artifacts = if coverage & super::ARTIFACTS != 0 {
        serde_json::to_string(&inventory.artifacts).map_err(encode_error)?
    } else {
        "[]".into()
    };
    db.execute(
        "INSERT INTO inventory_snapshots
             (uid, captured_at, captured_nanos, source, good_version, weapons_json, artifacts_json, coverage)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            uid,
            captured_at.original,
            captured_at.nanos,
            source,
            version,
            weapons,
            artifacts,
            coverage,
        ],
    )
    .map_err(|error| database_error(&error))?;
    if coverage & super::MATERIALS != 0 {
        save_materials(db, db.last_insert_rowid(), inventory)?;
    }
    Ok(())
}

fn save_materials(db: &Connection, id: i64, inventory: &Inventory) -> Result<(), String> {
    let save = || -> rusqlite::Result<()> {
        let mut catalog = db.prepare(
            "INSERT INTO material_catalog (good_key) VALUES (?1) ON CONFLICT(good_key) DO NOTHING",
        )?;
        let mut amount = db.prepare(
            "INSERT INTO inventory_materials (snapshot_id, good_key, quantity) VALUES (?1, ?2, ?3)",
        )?;
        for (key, quantity) in &inventory.materials {
            catalog.execute([key])?;
            amount.execute(params![id, key, quantity])?;
        }
        Ok(())
    };
    save().map_err(|error| database_error(&error))
}

fn backup(path: &Path, db: &Connection, preview: &Preview) -> Result<Vec<PathBuf>, String> {
    let uid = &preview.uid;
    let unique = windows::core::GUID::new().map_err(|_| "无法生成背包备份文件名")?;
    let destination = path
        .with_file_name("inventory-backups")
        .join(format!("{uid}-{unique:?}.json"));
    if preview.replaced.is_empty() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "format": "genshin-go-inventory-backup",
            "version": 1,
            "uid": uid,
            "snapshot": null
        }))
        .map_err(|error| format!("无法编码背包备份：{error}"))?;
        atomic_write(&destination, &bytes)?;
        return Ok(vec![destination]);
    }
    let mut written = Vec::with_capacity(preview.replaced.len());
    for (index, header) in preview.replaced.iter().enumerate() {
        let bytes = inventory(db, uid, header.id)?.encode(
            header.version,
            &header.source,
            uid,
            &header.captured_at,
            header.coverage,
        )?;
        let file = if index == 0 {
            destination.clone()
        } else {
            destination.with_file_name(format!("{uid}-{unique:?}-{}.json", index + 1))
        };
        atomic_write(&file, &bytes)?;
        written.push(file);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{account_store, accounts::Authorization};
    use std::fs;

    #[test]
    fn synced_materials_check_authorization_reject_empty_and_flag_historical_writes()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!(
            "genshin-go-inv-sync-{}-{}",
            std::process::id(),
            windows::core::GUID::new()?.data1
        ));
        fs::create_dir_all(&dir)?;
        let path = dir.join("launcher.sqlite3");
        let saved = account_store::save(
            &path,
            Authorization {
                account_id: "10001".into(),
                cookie_header: "account_id=10001; cookie_token=t; ltuid=10001; ltoken=l".into(),
                community_cookie: None,
            },
            "123456789",
            None,
        )?;
        let account = saved.account.ok_or("missing saved account")?;

        let t2 = CaptureTime::parse("2026-11-22T12:00:00Z")?;
        assert!(
            save_synced_materials(&path, &account, &t2, "MiYouShe-Calculator", BTreeMap::new())
                .is_err()
        );

        let (effect, backups) = save_synced_materials(
            &path,
            &account,
            &t2,
            "MiYouShe-Calculator",
            BTreeMap::from([("Mora".into(), 500)]),
        )?;
        assert_eq!(effect, ImportEffect::Latest);
        assert_eq!(backups.len(), 1);
        assert!(backups[0].is_file());

        let t1 = CaptureTime::parse("2026-11-21T12:00:00Z")?;
        let (older_effect, _) = save_synced_materials(
            &path,
            &account,
            &t1,
            "MiYouShe-Calculator",
            BTreeMap::from([("Mora".into(), 100)]),
        )?;
        assert_eq!(older_effect, ImportEffect::Historical);
        let latest = latest_materials(&path, &account.uid)?.ok_or("missing latest materials")?;
        assert_eq!(latest.quantities.get("Mora"), Some(&500));

        let stale = VerifiedAccount {
            revision: "stale-revision".into(),
            ..account
        };
        let t3 = CaptureTime::parse("2026-11-23T12:00:00Z")?;
        assert!(
            save_synced_materials(
                &path,
                &stale,
                &t3,
                "MiYouShe-Calculator",
                BTreeMap::from([("Mora".into(), 900)]),
            )
            .is_err()
        );

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
