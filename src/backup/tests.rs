use super::*;
use crate::account_store::{load, save, select_uid};
use crate::accounts::Authorization;
use serde_json::{Value, json};

const UID: &str = "123450020";
const EMPTY_UID: &str = "123450021";
const EXPECTED_FILE: &str = "seed.json";

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "genshin-go-backup-{:?}",
            windows::core::GUID::new()?
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn wish(id: &str, item_id: &str) -> Value {
    json!({
        "id": id, "item_id": item_id, "time": "2025-10-09 08:00:00",
        "gacha_type": "400", "uigf_gacha_type": "301", "rank_type": "5"
    })
}

fn progress(status: u8, current: u32, timestamp: i64) -> Value {
    json!({ "id": 80001, "status": status, "current": current, "timestamp": timestamp })
}

fn write_seed(path: &Path, wishes: &[Value], achievements: &[Value]) -> std::io::Result<()> {
    fs::write(
        path,
        json!({
            "format": "genshin-go-backup", "version": 1,
            "uids": [{"uid": UID, "wishes": wishes, "achievements": achievements}]
        })
        .to_string(),
    )
}

fn contents(file: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(file)?)?)
}

#[test]
fn migrates_all_uid_spaces_and_records_between_databases_without_authorization()
-> Result<(), Box<dyn std::error::Error>> {
    let test = TestDirectory::new()?;
    let original = test.0.join("original/launcher.sqlite3");
    let migrated = test.0.join("migrated/launcher.sqlite3");
    let archive = test.0.join(EXPECTED_FILE);
    let wishes = [wish("1000000000000000001", "10000002")];
    let achievements = [progress(3, 30, 1_760_000_000)];
    write_seed(&archive, &wishes, &achievements)?;
    restore(&original, &preview(&original, &archive)?)?;
    select_uid(&original, EMPTY_UID, true)?;
    let original_profile = save(
        &original,
        Authorization {
            account_id: "98765".into(),
            cookie_header: "fixture-passport-cookie".into(),
            community_cookie: Some("fixture-community-cookie".into()),
        },
        UID,
        None,
    )?;
    assert!(original_profile.account.is_some());

    let archive = export(&original)?;
    let expected = json!({
        "format": "genshin-go-backup", "version": 1,
        "uids": [
            {"uid": UID, "wishes": wishes, "achievements": achievements},
            {"uid": EMPTY_UID, "wishes": [], "achievements": []}
        ]
    });
    assert_eq!(contents(&archive)?, expected);
    let pending = preview(&migrated, &archive)?;
    assert!(load(&migrated)? == Profile::default());
    let (before, _) = restore(&migrated, &pending)?;
    assert_eq!(contents(&before)?["uids"], json!([]));
    let profile = load(&migrated)?;
    assert_eq!(profile.uids, [UID, EMPTY_UID]);
    assert_eq!(profile.active_uid.as_deref(), Some(UID));
    assert!(profile.account.is_none());
    assert_eq!(contents(&export(&migrated)?)?, expected);
    assert!(!preview(&migrated, &archive)?.has_changes());
    Ok(())
}

#[test]
fn merges_records_only_after_a_current_preview_and_successful_backup()
-> Result<(), Box<dyn std::error::Error>> {
    let test = TestDirectory::new()?;
    let target = test.0.join("target/launcher.sqlite3");
    let archive = test.0.join(EXPECTED_FILE);
    let saved_wish = wish("10", "10000002");
    let saved_progress = progress(2, 30, 1_760_000_000);
    write_seed(
        &archive,
        std::slice::from_ref(&saved_wish),
        std::slice::from_ref(&saved_progress),
    )?;
    restore(&target, &preview(&target, &archive)?)?;

    let additions = wish("11", "11401");
    write_seed(
        &archive,
        &[wish("10", "10000003"), additions.clone()],
        &[progress(3, 12, 1_760_000_100)],
    )?;
    let stale = preview(&target, &archive)?;
    assert_eq!(
        stale.entries[0].summaries(),
        [
            format!("UID {UID} · 祈愿新增 1，重复 0，冲突 1"),
            format!("UID {UID} · 成就新增 0，更新 1，重复 0，冲突 1"),
        ]
    );
    let another_archive = test.0.join("another.json");
    write_seed(&another_archive, &[], &[progress(2, 40, 1_760_000_050)])?;
    restore(&target, &preview(&target, &another_archive)?)?;
    assert_eq!(
        restore(&target, &stale).err().as_deref(),
        Some("本机 UID 记录已变更，请重新预览")
    );

    let pending = preview(&target, &archive)?;
    let blocked = target.with_file_name("uid-backups");
    let backup_dir = target.with_file_name("saved-backups");
    fs::rename(&blocked, &backup_dir)?;
    fs::write(&blocked, [])?;
    assert!(
        restore(&target, &pending)
            .err()
            .is_some_and(|error| error.starts_with("恢复前备份失败："))
    );
    fs::remove_file(&blocked)?;
    fs::rename(backup_dir, blocked)?;

    let (before, _) = restore(&target, &pending)?;
    assert_eq!(contents(&before)?["uids"][0]["wishes"], json!([saved_wish]));
    assert_eq!(
        contents(&before)?["uids"][0]["achievements"],
        json!([progress(2, 40, 1_760_000_000)])
    );
    let merged = contents(&export(&target)?)?;
    assert_eq!(merged["uids"][0]["wishes"], json!([additions, saved_wish]));
    assert_eq!(
        merged["uids"][0]["achievements"],
        json!([progress(3, 40, 1_760_000_000)])
    );
    Ok(())
}

#[test]
fn rejects_malformed_or_unsupported_backup_files() -> Result<(), Box<dyn std::error::Error>> {
    let test = TestDirectory::new()?;
    let archive = test.0.join(EXPECTED_FILE);
    write_seed(&archive, &[], &[json!([80001, 1, 3, 1])])?;
    assert!(
        Backup::read(&archive)
            .err()
            .is_some_and(|error| error.starts_with("整体备份格式无效："))
    );

    let mut unsupported = contents(&archive)?;
    unsupported["version"] = json!(2);
    unsupported["uids"][0]["achievements"] = json!([]);
    fs::write(&archive, serde_json::to_vec(&unsupported)?)?;
    assert_eq!(
        Backup::read(&archive).err().as_deref(),
        Some("不支持整体备份版本 2")
    );

    let standard = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/uiaf-v1.1.json");
    assert!(
        Backup::read(&standard)
            .err()
            .is_some_and(|error| error.starts_with("整体备份格式无效："))
    );
    Ok(())
}
