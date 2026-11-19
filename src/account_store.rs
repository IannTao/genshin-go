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

const SCHEMA_VERSION: u32 = 3;

fn database_error(error: &rusqlite::Error) -> String {
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

fn open(path: &Path) -> Result<Connection, String> {
    fs::create_dir_all(path.parent().ok_or("本机数据目录无效")?)
        .map_err(|error| format!("无法创建本机数据目录：{error}"))?;
    let mut db = Connection::open(path).map_err(|error| database_error(&error))?;
    db.pragma_update(None, "foreign_keys", true)
        .map_err(|error| database_error(&error))?;
    db.pragma_update(None, "secure_delete", true)
        .map_err(|error| database_error(&error))?;
    if schema_version(&db)? == SCHEMA_VERSION {
        return Ok(db);
    }
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
            ALTER TABLE account_authorization ADD COLUMN revision TEXT NOT NULL DEFAULT '';
            UPDATE account_authorization SET revision = lower(hex(randomblob(16)));
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
    migration
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(|error| database_error(&error))?;
    migration.commit().map_err(|error| database_error(&error))?;
    Ok(db)
}

fn snapshot(db: &Connection) -> rusqlite::Result<Profile> {
    let mut query = db.prepare("SELECT uid FROM uid_spaces ORDER BY uid")?;
    let uids = query
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
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

/// Reads a consistent snapshot of the local UID spaces and account metadata.
pub(crate) fn load(path: &Path) -> Result<Profile, String> {
    let mut db = open(path)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    snapshot(&read).map_err(|error| database_error(&error))
}

/// Sets the active UID and optionally creates its data space in the same transaction.
pub(crate) fn select_uid(path: &Path, uid: &str, create: bool) -> Result<Profile, String> {
    validate_uid(uid)?;
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    if create {
        write
            .execute(
                "INSERT INTO uid_spaces (uid) VALUES (?1) ON CONFLICT(uid) DO NOTHING",
                [uid],
            )
            .map_err(|error| database_error(&error))?;
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

/// Loads and decrypts the Cookie for the specified saved account.
pub(crate) fn load_authorization(path: &Path, account_id: &str) -> Result<Authorization, String> {
    let mut encrypted: Vec<u8> = open(path)?
        .query_row(
            "SELECT protected_cookie FROM account_authorization WHERE singleton = 1 AND account_id = ?1",
            [account_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| database_error(&error))?
        .ok_or("账号授权不存在，请重新扫码")?;
    let plaintext = crypt(&mut encrypted, false)?;
    let authorization = Authorization {
        account_id: account_id.into(),
        cookie_header: String::from_utf8(plaintext).map_err(|_| "本机凭据损坏，请重新扫码")?,
    };
    authorization.validate()?;
    Ok(authorization)
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
    let mut plaintext = authorization.cookie_header.into_bytes();
    let encrypted = crypt(&mut plaintext, true);
    plaintext.fill(0);
    let encrypted = encrypted?;
    let mut db = open(path)?;
    let write = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| database_error(&error))?;
    if current_account(&write)
        .map_err(|error| database_error(&error))?
        .as_ref()
        != expected
    {
        return Err("本机授权已在另一启动器中更改，请重新打开后重试".into());
    }
    write
        .execute(
            "INSERT INTO uid_spaces (uid) VALUES (?1) ON CONFLICT(uid) DO NOTHING",
            [uid],
        )
        .map_err(|error| database_error(&error))?;
    write
        .execute(
            "INSERT INTO active_uid (singleton, uid) VALUES (1, ?1) ON CONFLICT(singleton) DO NOTHING",
            [uid],
        )
        .map_err(|error| database_error(&error))?;
    write
        .execute(
            "
        INSERT INTO account_authorization (singleton, account_id, uid, protected_cookie, revision)
        VALUES (1, ?1, ?2, ?3, lower(hex(randomblob(16))))
        ON CONFLICT(singleton) DO UPDATE SET
            protected_cookie = excluded.protected_cookie, revision = excluded.revision
    ",
            params![authorization.account_id, uid, encrypted],
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
        }
    }

    #[test]
    fn migrates_uid_spaces_and_preserves_their_separate_history()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = isolated_database()?;
        fs::create_dir_all(path.parent().ok_or("no directory")?)?;
        let prior = Connection::open(&path)?;
        prior.execute_batch("
            PRAGMA user_version = 1;
            CREATE TABLE uid_spaces (uid TEXT NOT NULL PRIMARY KEY);
            CREATE TABLE active_uid (singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                uid TEXT NOT NULL REFERENCES uid_spaces(uid));
            CREATE TABLE example_history (uid TEXT NOT NULL REFERENCES uid_spaces(uid), entry TEXT NOT NULL);
            INSERT INTO uid_spaces VALUES ('123456789'), ('987654321');
            INSERT INTO active_uid VALUES (1, '123456789');
            INSERT INTO example_history VALUES ('123456789', 'first'), ('987654321', 'second');
        ")?;
        drop(prior);
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
        assert_eq!(load(&path)?.uids, ["123456789"]);
        assert_eq!(
            load_authorization(&path, "10001")?.cookie_header,
            "test-secret-one"
        );
        db.execute_batch("DROP TRIGGER fail_renew;")?;
        save(
            &path,
            grant("10001", "test-secret-renewed"),
            "123456789",
            original,
        )?;
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
    fn removing_and_readding_authorization_leaves_each_history_intact()
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
        assert_eq!(
            db.query_row(
                "SELECT group_concat(entry, ',' ORDER BY uid) FROM example_history",
                [],
                |row| row.get::<_, String>(0)
            )?,
            "first,second"
        );
        drop(db);
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
            crate::account_service::verify_uid(&authorization)?,
            account.uid
        );
        Ok(())
    }
}
