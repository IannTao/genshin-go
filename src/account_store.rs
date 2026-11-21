//! Windows SQLite storage for UID data spaces and DPAPI-protected authorization.
use crate::accounts::Authorization;
use crate::profile::{Profile, VerifiedAccount, validate_uid};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{fs, path::Path};
use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    },
    core::w,
};

const SCHEMA_VERSION: u32 = 9;

/// Formats an error encountered while accessing the launcher database.
pub(crate) fn database_error(error: &rusqlite::Error) -> String {
    format!("本机 SQLite 操作失败：{error}")
}

fn schema_version(db: &Connection) -> Result<u32, String> {
    let version = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| database_error(&error))?;
    if version > SCHEMA_VERSION {
        return Err("此版本启动器无法读取较新的本机数据库".into());
    }
    Ok(version)
}

/// Opens and migrates the launcher database with foreign keys enabled.
pub(crate) fn open(path: &Path) -> Result<Connection, String> {
    fs::create_dir_all(path.parent().ok_or("本机数据目录无效")?)
        .map_err(|error| format!("无法创建本机数据目录：{error}"))?;
    // Multi-account releases stored all grants in separate DPAPI files. Their
    // absence completes cleanup without a marker or a change to SQLite data.
    for filename in ["accounts.dpapi", "accounts-passport.dpapi"] {
        match fs::remove_file(path.with_file_name(filename)) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(format!("无法清除旧版米游社授权（{filename}）：{error}")),
        }
    }
    let mut db = Connection::open(path).map_err(|error| database_error(&error))?;
    db.pragma_update(None, "foreign_keys", true)
        .map_err(|error| database_error(&error))?;
    db.pragma_update(None, "secure_delete", true)
        .map_err(|error| database_error(&error))?;
    if schema_version(&db)? < SCHEMA_VERSION {
        migrate(&mut db)?;
    }
    Ok(db)
}

fn migrate(db: &mut Connection) -> Result<(), String> {
    let migration = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let version = schema_version(&migration)?;
    if version < 1 {
        migration
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS uid_spaces (uid TEXT NOT NULL PRIMARY KEY);
            CREATE TABLE IF NOT EXISTS active_uid (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                uid TEXT NOT NULL REFERENCES uid_spaces(uid)
            );
        ",
            )
            .map_err(|error| database_error(&error))?;
    }
    if version < 2 {
        migration
            .execute_batch(
                "
            CREATE TABLE account_authorization (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                account_id TEXT NOT NULL,
                uid TEXT NOT NULL REFERENCES uid_spaces(uid),
                protected_cookie BLOB NOT NULL
            );
        ",
            )
            .map_err(|error| database_error(&error))?;
    }
    if version < 3 {
        // UID-owned business tables reference uid_history(uid) ON DELETE CASCADE.
        // Account authorization and the current selection reference uid_spaces.
        migration
            .execute_batch(
                "
            CREATE TABLE uid_history (
                uid TEXT NOT NULL PRIMARY KEY REFERENCES uid_spaces(uid)
            );
            INSERT INTO uid_history (uid) SELECT uid FROM uid_spaces;
            CREATE TRIGGER initialize_uid_history AFTER INSERT ON uid_spaces BEGIN
                INSERT INTO uid_history (uid) VALUES (NEW.uid);
            END;
        ",
            )
            .map_err(|error| database_error(&error))?;
    }
    if version < 4 {
        let device = windows::core::GUID::new().map_err(|_| "无法生成本机设备标识")?;
        migration
            .execute_batch(
                "CREATE TABLE passport_device (
                    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                    device_id TEXT NOT NULL
                );",
            )
            .map_err(|error| database_error(&error))?;
        migration
            .execute(
                "INSERT INTO passport_device (singleton, device_id) VALUES (1, ?1)",
                [format!("{device:?}")],
            )
            .map_err(|error| database_error(&error))?;
    }
    if version < 5 {
        migration
            .execute_batch(
                "ALTER TABLE account_authorization ADD COLUMN protected_community_cookie BLOB;",
            )
            .map_err(|error| database_error(&error))?;
    }
    if version < 6 {
        migration
            .execute_batch(crate::wishes::SCHEMA)
            .map_err(|error| database_error(&error))?;
    }
    if version < 7 {
        ensure_authorization_revision(&migration).map_err(|error| database_error(&error))?;
    }
    if version < 8 {
        migration
            .execute_batch(
                "CREATE TABLE achievement_records (
                    uid TEXT NOT NULL REFERENCES uid_history(uid) ON DELETE CASCADE,
                    achievement_id INTEGER NOT NULL,
                    record_json TEXT NOT NULL,
                    PRIMARY KEY (uid, achievement_id)
                );",
            )
            .map_err(|error| database_error(&error))?;
    }
    if version < 9 {
        migration
            .execute_batch(crate::scanner::SCHEMA)
            .map_err(|error| database_error(&error))?;
    }
    migration
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(|error| database_error(&error))?;
    migration.commit().map_err(|error| database_error(&error))
}

fn ensure_authorization_revision(db: &Connection) -> rusqlite::Result<()> {
    // Some databases reached version 6 without the authorization revision column.
    let has_revision = db.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM pragma_table_info('account_authorization')
            WHERE name = 'revision'
        )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !has_revision {
        db.execute_batch(
            "ALTER TABLE account_authorization ADD COLUMN revision TEXT NOT NULL DEFAULT '';
             UPDATE account_authorization SET revision = lower(hex(randomblob(16)));",
        )?;
    }
    Ok(())
}

/// Reads the device identifier assigned to this local database.
pub(crate) fn device_id(path: &Path) -> Result<String, String> {
    open(path)?
        .query_row(
            "SELECT device_id FROM passport_device WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| database_error(&error))
}

/// Lists every local UID data space, including spaces without records.
pub(crate) fn list_uids(db: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut query = db.prepare("SELECT uid FROM uid_spaces ORDER BY uid")?;
    query.query_map([], |row| row.get(0))?.collect()
}

/// Reads UID selection and account metadata from the supplied database view.
pub(crate) fn snapshot(db: &Connection) -> rusqlite::Result<Profile> {
    let uids = list_uids(db)?;
    let active_uid = db
        .query_row(
            "SELECT uid FROM active_uid WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(Profile {
        uids,
        active_uid,
        account: current_account(db)?,
    })
}

fn current_account(db: &Connection) -> rusqlite::Result<Option<VerifiedAccount>> {
    db.query_row(
        "SELECT account_id, uid, revision FROM account_authorization WHERE singleton = 1",
        [],
        |row| {
            Ok(VerifiedAccount {
                account_id: row.get(0)?,
                uid: row.get(1)?,
                revision: row.get(2)?,
            })
        },
    )
    .optional()
}

/// Checks whether the specified authorization still matches the saved revision.
pub(crate) fn is_current_authorization(
    path: &Path,
    account: &VerifiedAccount,
) -> Result<bool, String> {
    Ok(current_account(&open(path)?)
        .map_err(|error| database_error(&error))?
        .as_ref()
        == Some(account))
}

/// Reads a consistent snapshot of the local UID spaces and account metadata.
pub(crate) fn load(path: &Path) -> Result<Profile, String> {
    let mut db = open(path)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    snapshot(&read).map_err(|error| database_error(&error))
}

/// Creates a UID data space if it does not already exist.
pub(crate) fn ensure_uid_space(db: &Connection, uid: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO uid_spaces (uid) VALUES (?1) ON CONFLICT(uid) DO NOTHING",
        [uid],
    )?;
    Ok(())
}

/// Sets the active UID only if no UID has been selected yet.
pub(crate) fn ensure_active_uid(db: &Connection, uid: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO active_uid (singleton, uid) VALUES (1, ?1) ON CONFLICT(singleton) DO NOTHING",
        [uid],
    )?;
    Ok(())
}

/// Sets the active UID and optionally creates its data space in the same transaction.
pub(crate) fn select_uid(path: &Path, uid: &str, create: bool) -> Result<Profile, String> {
    validate_uid(uid)?;
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    if create {
        ensure_uid_space(&write, uid).map_err(|error| database_error(&error))?;
    }
    let selected = write
        .execute(
            "
        INSERT INTO active_uid (singleton, uid) SELECT 1, uid FROM uid_spaces WHERE uid = ?1
        ON CONFLICT(singleton) DO UPDATE SET uid = excluded.uid
    ",
            [uid],
        )
        .map_err(|error| database_error(&error))?;
    if selected == 0 {
        return Err("该 UID 数据空间不存在".into());
    }
    let state = snapshot(&write).map_err(|error| database_error(&error))?;
    write.commit().map_err(|error| database_error(&error))?;
    Ok(state)
}

/// Loads and decrypts the web Cookie for the specified saved account.
pub(crate) fn load_authorization(path: &Path, account_id: &str) -> Result<Authorization, String> {
    let encrypted: Vec<u8> = open(path)?
        .query_row(
            "SELECT protected_cookie FROM account_authorization WHERE singleton = 1 AND account_id = ?1",
            [account_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| database_error(&error))?
        .ok_or("账号授权不存在，请重新扫码")?;
    let authorization = Authorization {
        account_id: account_id.into(),
        cookie_header: decrypt_cookie(encrypted)?,
        community_cookie: None,
    };
    authorization.validate()?;
    Ok(authorization)
}

/// Holds the credential material needed for community sign-in.
pub(crate) struct CommunityGrant {
    /// Contains the web session used to read the day's task state.
    pub(crate) web_cookie: String,
    /// Contains the optional App session used to submit sign-in.
    pub(crate) community_cookie: Option<String>,
    /// Identifies the local Passport device used to obtain the session.
    pub(crate) device: String,
}

/// Loads the web and community Cookies and device ID for the exact saved authorization.
pub(crate) fn load_community_grant(
    path: &Path,
    account: &VerifiedAccount,
) -> Result<Option<CommunityGrant>, String> {
    let grant = open(path)?
        .query_row(
            "SELECT protected_cookie, protected_community_cookie, device_id
             FROM account_authorization CROSS JOIN passport_device
             WHERE account_authorization.singleton = 1 AND passport_device.singleton = 1
               AND account_id = ?1 AND uid = ?2 AND revision = ?3",
            params![account.account_id, account.uid, account.revision],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| database_error(&error))?;
    grant
        .map(|(web, community, device)| {
            Ok(CommunityGrant {
                web_cookie: decrypt_cookie(web)?,
                community_cookie: community.map(decrypt_cookie).transpose()?,
                device,
            })
        })
        .transpose()
}

fn decrypt_cookie(mut encrypted: Vec<u8>) -> Result<String, String> {
    let plaintext = crypt(&mut encrypted, false)?;
    String::from_utf8(plaintext).map_err(|_| "本机凭据损坏，请重新扫码".into())
}

fn encrypt_cookie(cookie: String) -> Result<Vec<u8>, String> {
    let mut plaintext = cookie.into_bytes();
    let encrypted = crypt(&mut plaintext, true);
    plaintext.fill(0);
    encrypted
}

/// Clears the business records for one UID and keeps its history available for future writes.
pub(crate) fn delete_uid_history(path: &Path, uid: &str) -> Result<Profile, String> {
    validate_uid(uid)?;
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let deleted = write
        .execute("DELETE FROM uid_history WHERE uid = ?1", [uid])
        .map_err(|error| database_error(&error))?;
    if deleted == 0 {
        return Err("该 UID 数据空间不存在".into());
    }
    write
        .execute("INSERT INTO uid_history (uid) VALUES (?1)", [uid])
        .map_err(|error| database_error(&error))?;
    let state = snapshot(&write).map_err(|error| database_error(&error))?;
    write.commit().map_err(|error| database_error(&error))?;
    Ok(state)
}

/// Atomically saves one verified account and creates its UID space if needed.
pub(crate) fn save(
    path: &Path,
    authorization: Authorization,
    uid: &str,
    expected: Option<&VerifiedAccount>,
) -> Result<Profile, String> {
    authorization.validate()?;
    validate_uid(uid)?;
    if expected
        .is_some_and(|account| account.account_id != authorization.account_id || account.uid != uid)
    {
        return Err("新授权与原账号或游戏 UID 不一致，已保留原授权".into());
    }
    let encrypted = encrypt_cookie(authorization.cookie_header)?;
    let community = authorization
        .community_cookie
        .map(encrypt_cookie)
        .transpose()?;
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    if current_account(&write)
        .map_err(|error| database_error(&error))?
        .as_ref()
        != expected
    {
        return Err("本机授权已在另一启动器中更改".into());
    }
    ensure_uid_space(&write, uid).map_err(|error| database_error(&error))?;
    ensure_active_uid(&write, uid).map_err(|error| database_error(&error))?;
    write
        .execute(
            "
        INSERT INTO account_authorization
            (singleton, account_id, uid, protected_cookie, protected_community_cookie, revision)
        VALUES (1, ?1, ?2, ?3, ?4, lower(hex(randomblob(16))))
        ON CONFLICT(singleton) DO UPDATE SET
            protected_cookie = excluded.protected_cookie,
            protected_community_cookie = excluded.protected_community_cookie,
            revision = excluded.revision
    ",
            params![authorization.account_id, uid, encrypted, community],
        )
        .map_err(|error| database_error(&error))?;
    let state = snapshot(&write).map_err(|error| database_error(&error))?;
    write.commit().map_err(|error| database_error(&error))?;
    Ok(state)
}

/// Deletes the matching protected authorization without changing any UID data.
pub(crate) fn remove_authorization(
    path: &Path,
    account: &VerifiedAccount,
) -> Result<Profile, String> {
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    let deleted = write
        .execute(
            "DELETE FROM account_authorization
             WHERE singleton = 1 AND account_id = ?1 AND uid = ?2 AND revision = ?3",
            params![account.account_id, account.uid, account.revision],
        )
        .map_err(|error| database_error(&error))?;
    if deleted == 0 {
        return Err("本机授权已更改，请重新打开启动器后重试".into());
    }
    let state = snapshot(&write).map_err(|error| database_error(&error))?;
    write.commit().map_err(|error| database_error(&error))?;
    Ok(state)
}

#[expect(
    unsafe_code,
    reason = "DPAPI returns a Windows-owned buffer that must be copied and freed"
)]
fn crypt(bytes: &mut [u8], protect: bool) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().map_err(|_| "凭据过大")?,
        pbData: bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // DPAPI owns the returned allocation. Copy it, clear it, then LocalFree it.
    unsafe {
        let result = if protect {
            CryptProtectData(
                &raw const input,
                w!("Genshin-Go account authorization"),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &raw mut output,
            )
        } else {
            CryptUnprotectData(
                &raw const input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &raw mut output,
            )
        };
        result.map_err(|_| {
            "Windows 无法保护或解密凭据，请使用原 Windows 用户；需要时重新扫码".to_owned()
        })?;
        let buffer = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let copied = buffer.to_vec();
        buffer.fill(0);
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(copied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_database() -> Result<std::path::PathBuf, windows::core::Error> {
        Ok(std::env::temp_dir()
            .join(format!(
                "genshin-go-sqlite-{:?}",
                windows::core::GUID::new()?
            ))
            .join("launcher.sqlite3"))
    }

    fn grant(account: &str, cookie: &str) -> Authorization {
        Authorization {
            account_id: account.into(),
            cookie_header: cookie.into(),
            community_cookie: None,
        }
    }

    #[test]
    fn repairs_v6_authorization_without_a_revision() -> Result<(), Box<dyn std::error::Error>> {
        let path = isolated_database()?;
        let db = open(&path)?;
        db.execute_batch(
            "ALTER TABLE account_authorization DROP COLUMN revision;
             DROP TABLE scanner_settings;
             DROP TABLE owned_characters;
             DROP TABLE achievement_records;
             PRAGMA user_version = 6;
             INSERT INTO uid_spaces VALUES ('123456789');
             INSERT INTO active_uid VALUES (1, '123456789');
             INSERT INTO account_authorization
                (singleton, account_id, uid, protected_cookie, protected_community_cookie)
             VALUES (1, '10001', '123456789', X'0102', X'0304');
             INSERT INTO wish_records (uid, record_id, drawn_at, record_json)
             VALUES ('123456789', '1', '2026-11-20 00:00:00', '{}');",
        )?;
        drop(db);

        let profile = load(&path)?;
        let account = profile.account.as_ref().ok_or("missing migrated account")?;
        assert_eq!(profile.uids, ["123456789"]);
        assert_eq!(profile.active_uid.as_deref(), Some(account.uid.as_str()));
        assert_eq!(account.account_id, "10001");
        assert_eq!(account.revision.len(), 32);
        assert!(load(&path)? == profile);

        let db = open(&path)?;
        let cookies: (Vec<u8>, Vec<u8>) = db.query_row(
            "SELECT protected_cookie, protected_community_cookie FROM account_authorization",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(cookies, (vec![1, 2], vec![3, 4]));
        let record: String = db.query_row(
            "SELECT record_id FROM wish_records WHERE uid = '123456789'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(record, "1");
        drop(db);
        fs::remove_file(&path)?;
        fs::remove_dir(path.parent().ok_or("no directory")?)?;
        Ok(())
    }

    #[test]
    fn community_cookie_is_protected_and_bound_to_the_saved_revision()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = isolated_database()?;
        let db = open(&path)?;
        db.execute_batch(
            "DROP TABLE wish_records;
             DROP TABLE scanner_settings;
             DROP TABLE owned_characters;
             DROP TABLE achievement_records;
             ALTER TABLE account_authorization DROP COLUMN protected_community_cookie;
             PRAGMA user_version = 4;",
        )?;
        db.execute("INSERT INTO uid_spaces (uid) VALUES ('123456789')", [])?;
        db.execute(
            "INSERT INTO account_authorization
                (singleton, account_id, uid, protected_cookie, revision)
             VALUES (1, '10001', '123456789', ?1, 'old')",
            [encrypt_cookie("original-web-cookie".into())?],
        )?;
        drop(db);
        let old = load(&path)?.account.ok_or("missing legacy account")?;
        assert_eq!(old.revision, "old");
        let legacy = load_community_grant(&path, &old)?.ok_or("legacy grant unavailable")?;
        assert_eq!(legacy.web_cookie, "original-web-cookie");
        assert!(legacy.community_cookie.is_none());

        let mut grant = grant("10001", "renewed-web-cookie");
        grant.community_cookie = Some("test-community-secret".into());
        let current = save(&path, grant, &old.uid, Some(&old))?
            .account
            .ok_or("missing renewed account")?;
        assert!(load_community_grant(&path, &old)?.is_none());
        let renewed = load_community_grant(&path, &current)?.ok_or("renewed grant unavailable")?;
        assert_eq!(
            renewed.community_cookie.as_deref(),
            Some("test-community-secret")
        );
        assert!(
            load_authorization(&path, "10001")?
                .community_cookie
                .is_none()
        );
        assert!(
            !fs::read(&path)?
                .windows(b"test-community-secret".len())
                .any(|bytes| bytes == b"test-community-secret")
        );

        remove_authorization(&path, &current)?;
        assert!(load_community_grant(&path, &current)?.is_none());
        fs::remove_file(&path)?;
        fs::remove_dir(path.parent().ok_or("no directory")?)?;
        Ok(())
    }

    fn seed_legacy_authorizations(path: &Path, version: u32, count: u32) -> Result<(), String> {
        let accounts: Vec<_> = (0..count)
            .map(|index| {
                serde_json::json!({
                    "account_id": (10001 + index).to_string(),
                    "cookie_header": "fixture-legacy-cookie",
                })
            })
            .collect();
        let mut plaintext = serde_json::to_vec(&serde_json::json!({
            "version": version,
            "accounts": accounts,
        }))
        .map_err(|error| error.to_string())?;
        let encrypted = crypt(&mut plaintext, true);
        plaintext.fill(0);
        fs::write(path, encrypted?).map_err(|error| error.to_string())
    }

    #[test]
    fn clears_legacy_authorizations_and_restores_a_new_scan_across_restarts()
    -> Result<(), Box<dyn std::error::Error>> {
        for count in [0, 1, 2] {
            let path = isolated_database()?;
            let directory = path.parent().ok_or("no directory")?;
            fs::create_dir_all(directory)?;
            let legacy = directory.join("accounts.dpapi");
            let passport = directory.join("accounts-passport.dpapi");
            seed_legacy_authorizations(&legacy, 1, count)?;
            seed_legacy_authorizations(&passport, 2, count)?;

            assert!(load(&path)?.account.is_none());
            assert!(!legacy.try_exists()?);
            assert!(!passport.try_exists()?);
            let saved = save(
                &path,
                grant("10001", "fixture-new-cookie"),
                "123456789",
                None,
            )?;
            for _ in 0..2 {
                assert!(load(&path)? == saved);
                assert_eq!(
                    load_authorization(&path, "10001")?.cookie_header,
                    "fixture-new-cookie"
                );
            }
            fs::remove_file(&path)?;
            fs::remove_dir(directory)?;
        }
        Ok(())
    }

    #[test]
    fn retries_interrupted_cleanup_without_losing_the_new_authorization()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::windows::fs::OpenOptionsExt;

        let path = isolated_database()?;
        let directory = path.parent().ok_or("no directory")?;
        fs::create_dir_all(directory)?;
        let legacy = directory.join("accounts.dpapi");
        let passport = directory.join("accounts-passport.dpapi");
        seed_legacy_authorizations(&legacy, 1, 1)?;
        seed_legacy_authorizations(&passport, 2, 2)?;
        let busy = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&passport)?;
        assert!(load(&path).is_err());
        assert!(!legacy.try_exists()?);
        drop(busy);

        assert!(load(&path)?.account.is_none());
        let saved = save(
            &path,
            grant("10001", "fixture-new-cookie"),
            "123456789",
            None,
        )?;
        seed_legacy_authorizations(&passport, 2, 2)?;
        assert!(load(&path)? == saved);
        assert!(!passport.try_exists()?);
        assert_eq!(
            load_authorization(&path, "10001")?.cookie_header,
            "fixture-new-cookie"
        );
        fs::remove_file(&path)?;
        fs::remove_dir(directory)?;
        Ok(())
    }

    #[test]
    fn reuses_the_device_id_across_connections_and_account_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = isolated_database()?;
        let original = device_id(&path)?;
        let independent = isolated_database()?;
        assert_ne!(original, device_id(&independent)?);
        let profile = save(&path, grant("10001", "test-cookie"), "123456789", None)?;
        let account = profile.account.ok_or("no saved account")?;
        remove_authorization(&path, &account)?;
        assert_eq!(device_id(&path)?, original);
        for database in [path, independent] {
            fs::remove_file(&database)?;
            fs::remove_dir(database.parent().ok_or("no directory")?)?;
        }
        Ok(())
    }

    fn seed_v1_uid_history(path: &Path) -> rusqlite::Result<()> {
        let prior = Connection::open(path)?;
        prior.execute_batch("
            PRAGMA user_version = 1;
            CREATE TABLE uid_spaces (uid TEXT NOT NULL PRIMARY KEY);
            CREATE TABLE active_uid (singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                uid TEXT NOT NULL REFERENCES uid_spaces(uid));
            CREATE TABLE example_history (uid TEXT NOT NULL REFERENCES uid_spaces(uid), entry TEXT NOT NULL);
            INSERT INTO uid_spaces VALUES ('123456789'), ('987654321');
            INSERT INTO active_uid VALUES (1, '123456789');
            INSERT INTO example_history VALUES ('123456789', 'first'), ('987654321', 'second');
        ")
    }

    #[test]
    fn migrates_uid_spaces_and_preserves_their_separate_history()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = isolated_database()?;
        let directory = path.parent().ok_or("no directory")?;
        fs::create_dir_all(directory)?;
        seed_v1_uid_history(&path)?;
        seed_legacy_authorizations(&directory.join("accounts-passport.dpapi"), 2, 2)?;
        let restored = load(&path)?;
        assert_eq!(restored.uids, ["123456789", "987654321"]);
        assert_eq!(restored.active_uid.as_deref(), Some("123456789"));
        assert!(restored.account.is_none());
        assert!(select_uid(&path, "555555555", false).is_err());
        select_uid(&path, "987654321", false)?;
        save(&path, grant("10001", "test-cookie"), "123456789", None)?;
        let db = open(&path)?;
        assert_eq!(
            db.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            SCHEMA_VERSION
        );
        let active_history = "SELECT entry FROM example_history JOIN active_uid USING (uid)";
        assert_eq!(
            db.query_row(active_history, [], |row| row.get::<_, String>(0))?,
            "second"
        );
        select_uid(&path, "123456789", false)?;
        assert_eq!(
            db.query_row(active_history, [], |row| row.get::<_, String>(0))?,
            "first"
        );
        drop(db);
        fs::remove_file(&path)?;
        fs::remove_dir(path.parent().ok_or("no directory")?)?;
        Ok(())
    }

    #[test]
    #[ignore = "seeds a new isolated directory for the native Windows upgrade validation"]
    fn prepare_windows_upgrade_validation() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::var_os("GENSHIN_GO_VALIDATION_LOCALAPPDATA")
            .ok_or("isolated validation directory missing")?;
        let directory = std::path::PathBuf::from(root).join("Genshin-Go");
        fs::create_dir(&directory)?;
        seed_v1_uid_history(&directory.join("launcher.sqlite3"))?;
        seed_legacy_authorizations(&directory.join("accounts-passport.dpapi"), 2, 2)?;
        Ok(())
    }

    #[test]
    fn concurrent_first_saves_restore_only_the_winning_authorization()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = isolated_database()?;
        let starting_line = std::sync::Barrier::new(2);
        let [first, second] = std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                starting_line.wait();
                save(&path, grant("10001", "first-cookie"), "123456789", None)
            });
            let second = scope.spawn(|| {
                starting_line.wait();
                save(&path, grant("10002", "second-cookie"), "987654321", None)
            });
            [first.join(), second.join()]
        });
        let (saved, rejected, cookie, error) = match (
            first.map_err(|_| "first launcher panicked")?,
            second.map_err(|_| "second launcher panicked")?,
        ) {
            (Ok(saved), Err(error)) => (saved, "10002", "first-cookie", error),
            (Err(error), Ok(saved)) => (saved, "10001", "second-cookie", error),
            (Err(first), Err(second)) => {
                return Err(
                    format!("neither launcher saved authorization: {first}; {second}").into(),
                );
            }
            (Ok(_), Ok(_)) => return Err("both launchers saved authorization".into()),
        };
        assert_eq!(error, "本机授权已在另一启动器中更改");
        let restored = load(&path)?;
        assert!(restored == saved);
        let account = restored.account.ok_or("no restored account")?;
        assert_eq!(restored.uids, [account.uid]);
        assert_eq!(
            load_authorization(&path, &account.account_id)?.cookie_header,
            cookie
        );
        assert!(load_authorization(&path, rejected).is_err());
        fs::remove_file(&path)?;
        fs::remove_dir(path.parent().ok_or("no directory")?)?;
        Ok(())
    }

    #[test]
    fn windows_protects_cookie_and_preserves_it_after_failed_renewal()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = isolated_database()?;
        let state = save(&path, grant("10001", "test-secret-one"), "123456789", None)?;
        assert_eq!(state.active_uid.as_deref(), Some("123456789"));
        let original = state.account.as_ref();
        assert!(
            save(
                &path,
                grant("10002", "test-secret-other"),
                "987654321",
                None
            )
            .is_err()
        );
        assert!(
            save(
                &path,
                grant("10001", "test-secret-other"),
                "987654321",
                original
            )
            .is_err()
        );
        let db = open(&path)?;
        db.execute_batch(
            "CREATE TRIGGER fail_renew BEFORE UPDATE ON account_authorization
            BEGIN SELECT RAISE(ABORT, 'fixture failure'); END;",
        )?;
        assert!(
            save(
                &path,
                grant("10001", "test-secret-other"),
                "123456789",
                original
            )
            .is_err()
        );
        assert!(load(&path)? == state);
        assert_eq!(
            load_authorization(&path, "10001")?.cookie_header,
            "test-secret-one"
        );
        db.execute_batch("DROP TRIGGER fail_renew;")?;
        let renewed = save(
            &path,
            grant("10001", "test-secret-renewed"),
            "123456789",
            original,
        )?;
        assert!(load(&path)? == renewed);
        let updated = renewed.account.as_ref().ok_or("no renewed account")?;
        let original = original.ok_or("no original account")?;
        assert_eq!(updated.account_id, original.account_id);
        assert_eq!(updated.uid, original.uid);
        assert_ne!(updated.revision, original.revision);
        assert_eq!(
            load_authorization(&path, "10001")?.cookie_header,
            "test-secret-renewed"
        );
        assert!(
            !fs::read(&path)?
                .windows(b"test-secret".len())
                .any(|bytes| bytes == b"test-secret")
        );
        drop(db);
        fs::remove_file(&path)?;
        fs::remove_dir(path.parent().ok_or("no directory")?)?;
        Ok(())
    }

    fn seed_history(db: &Connection) -> rusqlite::Result<()> {
        db.execute_batch(
            "CREATE TABLE example_history (
                uid TEXT NOT NULL REFERENCES uid_history(uid) ON DELETE CASCADE,
                entry TEXT NOT NULL
            );
            INSERT INTO example_history VALUES ('123456789', 'first'), ('987654321', 'second');",
        )
    }

    #[test]
    fn removing_and_switching_authorization_leaves_each_history_intact()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = isolated_database()?;
        select_uid(&path, "987654321", true)?;
        let state = save(
            &path,
            grant("10001", "test-original-cookie"),
            "123456789",
            None,
        )?;
        let account = state.account.ok_or("missing verified account")?;
        let db = open(&path)?;
        seed_history(&db)?;
        let encrypted: Vec<u8> = db.query_row(
            "SELECT protected_cookie FROM account_authorization",
            [],
            |row| row.get(0),
        )?;
        let other_account = VerifiedAccount {
            account_id: "10002".into(),
            ..account.clone()
        };
        assert!(remove_authorization(&path, &other_account).is_err());
        let removed = remove_authorization(&path, &account)?;
        assert!(removed.account.is_none());
        assert_eq!(removed.uids, ["123456789", "987654321"]);
        assert_eq!(removed.active_uid.as_deref(), Some("987654321"));
        assert!(load_authorization(&path, &account.account_id).is_err());
        assert!(
            !fs::read(&path)?
                .windows(encrypted.len())
                .any(|bytes| bytes == encrypted)
        );

        let added = save(&path, grant("10001", "test-new-cookie"), "123456789", None)?;
        let restored = added.account.ok_or("missing restored account")?;
        assert_eq!(restored.account_id, account.account_id);
        assert_eq!(restored.uid, account.uid);
        assert!(
            save(
                &path,
                grant("10001", "test-stale-cookie"),
                "123456789",
                Some(&account),
            )
            .is_err()
        );
        assert!(remove_authorization(&path, &account).is_err());
        assert_eq!(
            load_authorization(&path, "10001")?.cookie_header,
            "test-new-cookie"
        );
        assert_eq!(added.uids, removed.uids);
        assert_eq!(added.active_uid, removed.active_uid);
        drop(db);

        remove_authorization(&path, &restored)?;
        let switched = save(
            &path,
            grant("10002", "test-other-cookie"),
            "987654321",
            None,
        )?;
        let reopened = load(&path)?;
        assert!(reopened == switched);
        let current = reopened.account.ok_or("missing replacement account")?;
        assert_eq!(current.account_id, "10002");
        assert_eq!(current.uid, "987654321");
        assert_eq!(reopened.uids, removed.uids);
        assert_eq!(reopened.active_uid, removed.active_uid);
        assert!(load_authorization(&path, "10001").is_err());
        assert_eq!(
            load_authorization(&path, "10002")?.cookie_header,
            "test-other-cookie"
        );

        for (uid, entry) in [("123456789", "first"), ("987654321", "second")] {
            select_uid(&path, uid, false)?;
            let active_history = "SELECT entry FROM example_history JOIN active_uid USING (uid)";
            assert_eq!(
                open(&path)?.query_row(active_history, [], |row| row.get::<_, String>(0))?,
                entry
            );
        }
        fs::remove_file(&path)?;
        fs::remove_dir(path.parent().ok_or("no directory")?)?;
        Ok(())
    }

    #[test]
    fn clearing_one_history_preserves_other_records_and_the_saved_account()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = isolated_database()?;
        select_uid(&path, "987654321", true)?;
        let before = save(&path, grant("10001", "test-cookie"), "123456789", None)?;
        let db = open(&path)?;
        seed_history(&db)?;
        db.execute_batch(
            "CREATE TRIGGER fail_history_reset BEFORE INSERT ON uid_history
                BEGIN SELECT RAISE(ABORT, 'fixture failure'); END;",
        )?;
        let history = || {
            db.query_row(
                "SELECT group_concat(entry, ',' ORDER BY uid) FROM example_history",
                [],
                |row| row.get::<_, String>(0),
            )
        };
        assert!(delete_uid_history(&path, "123456789").is_err());
        assert_eq!(history()?, "first,second");
        db.execute_batch("DROP TRIGGER fail_history_reset;")?;
        assert!(delete_uid_history(&path, "555555555").is_err());
        assert!(delete_uid_history(&path, "invalid").is_err());
        assert_eq!(history()?, "first,second");

        let after = delete_uid_history(&path, "123456789")?;
        assert!(after == before);
        assert_eq!(history()?, "second");
        assert_eq!(
            load_authorization(&path, "10001")?.cookie_header,
            "test-cookie"
        );
        db.execute(
            "INSERT INTO example_history VALUES ('123456789', 'new')",
            [],
        )?;
        select_uid(&path, "123456789", true)?;
        assert_eq!(history()?, "new,second");
        drop(db);
        fs::remove_file(&path)?;
        fs::remove_dir(path.parent().ok_or("no directory")?)?;
        Ok(())
    }

    #[test]
    #[ignore = "requires an isolated Windows directory with a real QR authorization"]
    fn restored_passport_authorization_has_paired_cookie_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::var_os("GENSHIN_GO_VALIDATION_LOCALAPPDATA")
            .ok_or("isolated validation directory missing")?;
        let path = std::path::PathBuf::from(root)
            .join("Genshin-Go")
            .join("launcher.sqlite3");
        let account = load(&path)?.account.ok_or("saved authorization missing")?;
        let authorization = load_authorization(&path, &account.account_id)?;
        assert!(authorization.cookie_header.contains("ltoken="));
        assert!(authorization.cookie_header.contains("cookie_token="));
        assert!(authorization.cookie_header.contains("ltuid="));
        assert_eq!(
            crate::account_service::verify_uid(&authorization, &device_id(&path)?)?,
            account.uid
        );
        Ok(())
    }
}
