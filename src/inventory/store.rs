use super::{
    MaterialSnapshot,
    archive::{Archive, CaptureTime, Inventory},
};
use crate::{
    account_store::{database_error, open},
    native_environment::atomic_write,
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
    captured_nanos: i64,
}

/// Describes the visible effect of confirming a reviewed import.
#[derive(Clone, Copy, PartialEq, Eq)]
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
    pub current: Option<Header>,
    last_inserted: Option<i64>,
}

/// Lists a UID's snapshots in descending order of collection time.
pub(super) fn list(path: &Path, uid: &str) -> Result<Vec<Header>, String> {
    headers(&open(path)?, uid)
}

fn headers(db: &Connection, uid: &str) -> Result<Vec<Header>, String> {
    let mut query = db
        .prepare(
            "SELECT id, captured_at, source, good_version, captured_nanos
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
             WHERE uid = ?1 ORDER BY captured_nanos DESC LIMIT 1",
            [uid],
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

fn inventory(db: &Connection, uid: &str, id: i64) -> Result<Inventory, String> {
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
) -> Result<Preview, String> {
    let mut db = open(path)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    plan(&read, uid, archive, captured_at)
}

fn plan(
    db: &Connection,
    uid: &str,
    archive: &Archive,
    captured_at: &CaptureTime,
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
    let effect = if let Some(existing) = history
        .iter()
        .find(|entry| entry.captured_nanos == captured_at.nanos)
    {
        if inventory(db, uid, existing.id)? != archive.inventory {
            return Err("已有采集时间相同但内容不同的快照，请核对原始采集时间".into());
        }
        ImportEffect::Duplicate
    } else if history
        .first()
        .is_some_and(|latest| latest.captured_nanos > captured_at.nanos)
    {
        ImportEffect::Historical
    } else {
        ImportEffect::Latest
    };
    Ok(Preview {
        uid: uid.to_owned(),
        effect,
        last_inserted: history.iter().map(|header| header.id).max(),
        current: history.into_iter().next(),
    })
}

/// Rechecks the preview, saves a backup and inserts a complete snapshot atomically.
pub(super) fn import(
    path: &Path,
    archive: &Archive,
    captured_at: &CaptureTime,
    preview: &Preview,
) -> Result<PathBuf, String> {
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let actual = plan(&write, &preview.uid, archive, captured_at)?;
    if &actual != preview {
        return Err("背包快照已变化，请重新预览".into());
    }
    if actual.effect == ImportEffect::Duplicate {
        return Err("这份背包快照已保存".into());
    }
    // Ensure the new snapshot fits the same file limit when backed up later.
    archive.inventory.encode(
        archive.version,
        &archive.source,
        &actual.uid,
        &captured_at.original,
    )?;
    let backup =
        backup(path, &write, &actual).map_err(|error| format!("导入前备份失败：{error}"))?;
    let encode_error = |error| format!("无法编码背包记录：{error}");
    write
        .execute(
            "INSERT INTO inventory_snapshots
             (uid, captured_at, captured_nanos, source, good_version, weapons_json, artifacts_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                actual.uid,
                captured_at.original,
                captured_at.nanos,
                archive.source,
                archive.version,
                serde_json::to_string(&archive.inventory.weapons).map_err(encode_error)?,
                serde_json::to_string(&archive.inventory.artifacts).map_err(encode_error)?,
            ],
        )
        .map_err(|error| database_error(&error))?;
    save_materials(&write, write.last_insert_rowid(), &archive.inventory)?;
    write.commit().map_err(|error| database_error(&error))?;
    Ok(backup)
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

fn backup(path: &Path, db: &Connection, preview: &Preview) -> Result<PathBuf, String> {
    let uid = &preview.uid;
    let unique = windows::core::GUID::new().map_err(|_| "无法生成背包备份文件名")?;
    let destination = path
        .with_file_name("inventory-backups")
        .join(format!("{uid}-{unique:?}.json"));
    let bytes = if let Some(header) = &preview.current {
        inventory(db, uid, header.id)?.encode(
            header.version,
            &header.source,
            uid,
            &header.captured_at,
        )?
    } else {
        serde_json::to_vec(&serde_json::json!({
            "format": "genshin-go-inventory-backup",
            "version": 1,
            "uid": uid,
            "snapshot": null
        }))
        .map_err(|error| format!("无法编码背包备份：{error}"))?
    };
    atomic_write(&destination, &bytes)?;
    Ok(destination)
}
