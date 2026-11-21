use super::protocol::{Character, Snapshot};
use crate::{
    account_store::{self, database_error},
    profile::Profile,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use std::path::{Path, PathBuf};

/// Identifies the selected capture program and its local management port.
pub(super) struct Settings {
    pub program: PathBuf,
    pub port: u16,
}

impl Settings {
    /// Loads the current configuration when synchronization is enabled.
    pub(super) fn load(path: &Path) -> Result<Option<Self>, String> {
        account_store::open(path)?
            .query_row(
                "SELECT program, port FROM scanner_settings WHERE singleton=1",
                [],
                |row| {
                    Ok(Self {
                        program: PathBuf::from(row.get::<_, String>(0)?),
                        port: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(|e| database_error(&e))
    }

    /// Enables synchronization with the selected Windows capture program.
    pub(super) fn save(&self, path: &Path) -> Result<(), String> {
        if !self.program.is_absolute()
            || !self.program.is_file()
            || !self
                .program
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("GGScanner.exe"))
        {
            return Err("请选择已有的 GGScanner.exe 抓包版".into());
        }
        account_store::open(path)?
            .execute(
                "INSERT INTO scanner_settings VALUES (1, ?1, ?2)
             ON CONFLICT(singleton) DO UPDATE SET program=excluded.program, port=excluded.port",
                params![self.program.to_str().ok_or("程序路径无效")?, self.port],
            )
            .map_err(|e| database_error(&e))?;
        Ok(())
    }

    /// Disables synchronization for subsequent launches.
    pub(super) fn disable(path: &Path) -> Result<(), String> {
        account_store::open(path)?
            .execute("DELETE FROM scanner_settings WHERE singleton=1", [])
            .map_err(|e| database_error(&e))?;
        Ok(())
    }
}

/// Commits a newer complete roster and its timestamps to the verified UID.
pub(super) fn save_roster(
    path: &Path,
    snapshot: &Snapshot,
    characters: &[Character],
    received: i64,
) -> Result<Option<Profile>, String> {
    let mut db = account_store::open(path)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| database_error(&e))?;
    let uid = snapshot.uid();
    let json = serde_json::to_string(characters).map_err(|e| e.to_string())?;
    account_store::ensure_uid_space(&tx, uid).map_err(|e| database_error(&e))?;
    let changed = tx.execute(
        "INSERT INTO owned_characters (uid, session, revision, source_ms, received_ms, characters)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(uid) DO UPDATE SET session=excluded.session, revision=excluded.revision,
         source_ms=excluded.source_ms, received_ms=excluded.received_ms, characters=excluded.characters
         WHERE (session=excluded.session AND revision<excluded.revision AND source_ms<=excluded.source_ms)
            OR (session<>excluded.session AND source_ms<excluded.source_ms)",
        params![uid, snapshot.session_id, snapshot.revision, snapshot.updated_at, received, json],
    ).map_err(|e| database_error(&e))?;
    if changed == 0 {
        return Ok(None);
    }
    account_store::ensure_active_uid(&tx, uid).map_err(|e| database_error(&e))?;
    let profile = account_store::snapshot(&tx).map_err(|e| database_error(&e))?;
    tx.commit().map_err(|e| database_error(&e))?;
    Ok(Some(profile))
}

/// Contains the saved character records and their source and receipt times.
pub(super) struct Roster {
    pub source: i64,
    pub received: i64,
    pub characters: Vec<Character>,
}

/// Loads the latest saved roster for one UID.
pub(super) fn roster(path: &Path, uid: &str) -> Result<Option<Roster>, String> {
    let row = account_store::open(path)?
        .query_row(
            "SELECT source_ms, received_ms, characters FROM owned_characters WHERE uid=?1",
            [uid],
            |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, String>(2)?)),
        )
        .optional()
        .map_err(|e| database_error(&e))?;
    row.map(|(source, received, json)| {
        let characters = serde_json::from_str(&json).map_err(|_| "本机角色记录无效".to_owned())?;
        Ok(Roster {
            source,
            received,
            characters,
        })
    })
    .transpose()
}
