//! Local install, manual update, uninstall, and restart-verifiable state for curated plugins.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const STATE_FILE_NAME: &str = "installed.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct InstalledPlugin {
    plugin_id: String,
    pub(super) version: String,
    content_sha256: String,
    source_dir: String,
    files: Vec<String>,
}

impl InstalledPlugin {
    pub(super) fn short_hash(&self) -> &str {
        self.content_sha256.get(..8).unwrap_or(&self.content_sha256)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Describes whether the managed installation matches its saved manifest.
pub(super) enum InstallStatus {
    NotInstalled,
    Installed(InstalledPlugin),
    Corrupted {
        record: InstalledPlugin,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum UpdateOutcome {
    UpToDate(InstalledPlugin),
    Updated(InstalledPlugin),
}

pub(super) fn inspect(
    plugins_root: &Path,
    install_subpath: &str,
    plugin_id: &str,
    required_files: &[String],
) -> Result<InstallStatus, String> {
    let target_dir = managed_dir(plugins_root, install_subpath)?;
    let state_path = target_dir.join(STATE_FILE_NAME);
    if !state_path.exists() {
        return Ok(InstallStatus::NotInstalled);
    }
    let bytes = fs::read(&state_path).map_err(|error| format!("读取已安装状态失败：{error}"))?;
    let record: InstalledPlugin =
        serde_json::from_slice(&bytes).map_err(|error| format!("已安装状态文件损坏：{error}"))?;
    if record.plugin_id != plugin_id {
        return Err(format!(
            "已安装状态标识不匹配：期望 {plugin_id}，实际 {}",
            record.plugin_id
        ));
    }
    if record.files.len() != required_files.len()
        || required_files
            .iter()
            .any(|file| !record.files.contains(file))
    {
        return Ok(InstallStatus::Corrupted {
            record,
            reason: "受管文件清单与目录要求不一致".into(),
        });
    }

    let mut missing = Vec::new();
    for file in &record.files {
        validate_file_name(file)?;
        if !target_dir.join(file).is_file() {
            missing.push(file.clone());
        }
    }
    if !missing.is_empty() {
        return Ok(InstallStatus::Corrupted {
            record,
            reason: format!("缺少 {}", missing.join("、")),
        });
    }

    let mut manifest_input = Vec::new();
    for file in &record.files {
        let bytes = fs::read(target_dir.join(file))
            .map_err(|error| format!("读取受管文件 {file} 失败：{error}"))?;
        append_file_digest(&mut manifest_input, file, &bytes);
    }
    if sha256_hex(&manifest_input) != record.content_sha256 {
        return Ok(InstallStatus::Corrupted {
            record,
            reason: "受管文件内容与安装时的摘要不一致".into(),
        });
    }
    Ok(InstallStatus::Installed(record))
}

pub(super) fn install(
    plugins_root: &Path,
    install_subpath: &str,
    plugin_id: &str,
    version: &str,
    source_dir: &Path,
    required_files: &[String],
) -> Result<InstalledPlugin, String> {
    let target_dir = managed_dir(plugins_root, install_subpath)?;
    let staged = stage_from_source(source_dir, &target_dir, required_files)?;
    commit_record(
        &target_dir,
        plugin_id,
        version,
        source_dir,
        required_files,
        &staged,
    )
}

pub(super) fn update(
    plugins_root: &Path,
    install_subpath: &str,
    plugin_id: &str,
    version: &str,
    source_dir: &Path,
    required_files: &[String],
) -> Result<UpdateOutcome, String> {
    let current = match inspect(plugins_root, install_subpath, plugin_id, required_files)? {
        InstallStatus::Installed(record) => record,
        InstallStatus::NotInstalled => return Err("插件尚未安装，请先执行安装".into()),
        InstallStatus::Corrupted { reason, .. } => {
            return Err(format!("当前安装损坏（{reason}），请先重新安装"));
        }
    };

    let target_dir = managed_dir(plugins_root, install_subpath)?;
    let staged = stage_from_source(source_dir, &target_dir, required_files)?;
    if current.version == version && current.content_sha256 == staged.content_sha256 {
        return Ok(UpdateOutcome::UpToDate(current));
    }

    let record = commit_record(
        &target_dir,
        plugin_id,
        version,
        source_dir,
        required_files,
        &staged,
    )?;
    Ok(UpdateOutcome::Updated(record))
}

fn commit_record(
    target_dir: &Path,
    plugin_id: &str,
    version: &str,
    source_dir: &Path,
    required_files: &[String],
    staged: &StagedPayload,
) -> Result<InstalledPlugin, String> {
    let record = InstalledPlugin {
        plugin_id: plugin_id.to_owned(),
        version: version.to_owned(),
        content_sha256: staged.content_sha256.clone(),
        source_dir: source_dir.display().to_string(),
        files: required_files.to_vec(),
    };
    commit_staged(target_dir, &staged.files, &record)?;
    Ok(record)
}

pub(super) fn uninstall(
    plugins_root: &Path,
    install_subpath: &str,
    plugin_id: &str,
    required_files: &[String],
) -> Result<(), String> {
    let target_dir = managed_dir(plugins_root, install_subpath)?;
    let state_path = target_dir.join(STATE_FILE_NAME);
    let mut managed_files: Vec<String> = required_files.to_vec();

    if state_path.exists()
        && let Ok(bytes) = fs::read(&state_path)
        && let Ok(record) = serde_json::from_slice::<InstalledPlugin>(&bytes)
        && record.plugin_id == plugin_id
    {
        for file in record.files {
            if !managed_files.contains(&file) {
                managed_files.push(file);
            }
        }
    }

    // Remove managed files before removing the state manifest so a locked file
    // cannot leave an uninstalled status behind on restart.
    for file in &managed_files {
        validate_file_name(file)?;
        let path = target_dir.join(file);
        if path.exists() {
            fs::remove_file(&path).map_err(|error| format!("删除受管文件 {file} 失败：{error}"))?;
        }
    }

    if state_path.exists() {
        fs::remove_file(&state_path).map_err(|error| format!("删除安装状态文件失败：{error}"))?;
    }

    if target_dir.is_dir() {
        let is_empty = fs::read_dir(&target_dir)
            .map_err(|error| format!("检查插件目录失败：{error}"))?
            .next()
            .is_none();
        if is_empty {
            fs::remove_dir(&target_dir).map_err(|error| format!("清理空插件目录失败：{error}"))?;
        }
    }
    Ok(())
}

struct StagedPayload {
    files: Vec<(String, Vec<u8>)>,
    content_sha256: String,
}

fn stage_from_source(
    source_dir: &Path,
    target_dir: &Path,
    required_files: &[String],
) -> Result<StagedPayload, String> {
    if source_dir.as_os_str().is_empty() {
        return Err("请先填写本地插件来源目录".into());
    }
    if !source_dir.is_dir() {
        return Err(format!("本地来源目录不存在：{}", source_dir.display()));
    }
    if same_directory(source_dir, target_dir) {
        return Err("本地来源目录不能与插件管理目录相同".into());
    }
    if required_files.is_empty() {
        return Err("插件目录未声明任何受管文件".into());
    }

    let mut files = Vec::with_capacity(required_files.len());
    let mut manifest_input = Vec::new();
    for file in required_files {
        validate_file_name(file)?;
        let source_path = source_dir.join(file);
        if !source_path.is_file() {
            return Err(format!("来源目录缺少必需文件：{file}"));
        }
        let bytes =
            fs::read(&source_path).map_err(|error| format!("读取来源文件 {file} 失败：{error}"))?;
        append_file_digest(&mut manifest_input, file, &bytes);
        files.push((file.clone(), bytes));
    }

    Ok(StagedPayload {
        files,
        content_sha256: sha256_hex(&manifest_input),
    })
}

fn append_file_digest(manifest_input: &mut Vec<u8>, file: &str, bytes: &[u8]) {
    manifest_input.extend_from_slice(file.as_bytes());
    manifest_input.push(b':');
    manifest_input.extend_from_slice(sha256_hex(bytes).as_bytes());
    manifest_input.push(b'\n');
}

fn commit_staged(
    target_dir: &Path,
    files: &[(String, Vec<u8>)],
    record: &InstalledPlugin,
) -> Result<(), String> {
    fs::create_dir_all(target_dir).map_err(|error| format!("创建插件管理目录失败：{error}"))?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let staging_dir = target_dir.join(format!(".staging-{}-{unique}", std::process::id()));
    let backup_dir = target_dir.join(format!(".backup-{}-{unique}", std::process::id()));
    fs::create_dir(&staging_dir).map_err(|error| format!("创建暂存目录失败：{error}"))?;
    fs::create_dir(&backup_dir).map_err(|error| {
        let _ = fs::remove_dir_all(&staging_dir);
        format!("创建备份目录失败：{error}")
    })?;

    let mut replaced: Vec<(String, bool)> = Vec::new();
    let result = (|| {
        for (name, bytes) in files {
            fs::write(staging_dir.join(name), bytes)
                .map_err(|error| format!("写入暂存文件 {name} 失败：{error}"))?;
        }
        let state_bytes = serde_json::to_vec_pretty(record)
            .map_err(|error| format!("序列化安装状态失败：{error}"))?;
        fs::write(staging_dir.join(STATE_FILE_NAME), &state_bytes)
            .map_err(|error| format!("写入暂存状态失败：{error}"))?;

        for (name, _) in files {
            let had_backup = swap_in_file(target_dir, &staging_dir, &backup_dir, name)?;
            replaced.push((name.clone(), had_backup));
        }
        let had_state_backup =
            swap_in_file(target_dir, &staging_dir, &backup_dir, STATE_FILE_NAME)?;
        replaced.push((STATE_FILE_NAME.to_owned(), had_state_backup));
        Ok(())
    })();

    if result.is_err() {
        for (name, had_backup) in replaced.into_iter().rev() {
            let target = target_dir.join(&name);
            let _ = fs::remove_file(&target);
            if had_backup {
                let _ = fs::rename(backup_dir.join(&name), &target);
            }
        }
    }
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_dir_all(&backup_dir);
    result
}

fn swap_in_file(
    target_dir: &Path,
    staging_dir: &Path,
    backup_dir: &Path,
    name: &str,
) -> Result<bool, String> {
    let target = target_dir.join(name);
    let backup = backup_dir.join(name);
    let had_backup = if target.exists() {
        fs::rename(&target, &backup)
            .map_err(|error| format!("备份原有文件 {name} 失败：{error}"))?;
        true
    } else {
        false
    };
    if let Err(error) = fs::rename(staging_dir.join(name), &target) {
        if had_backup {
            let _ = fs::rename(&backup, &target);
        }
        return Err(format!("写入目标文件 {name} 失败：{error}"));
    }
    Ok(had_backup)
}

fn managed_dir(plugins_root: &Path, install_subpath: &str) -> Result<PathBuf, String> {
    super::validate_install_subpath("plugin", install_subpath)?;
    Ok(plugins_root.join(install_subpath))
}

pub(super) fn validate_file_name(name: &str) -> Result<(), String> {
    let mut components = Path::new(name).components();
    let valid = !name.is_empty()
        && !name.contains('\\')
        && !name.contains('/')
        && !name.contains(':')
        && matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none();
    if !valid {
        return Err(format!("受管文件名不合法：{name}"));
    }
    Ok(())
}

fn same_directory(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(a), Ok(b)) => a == b,
        _ => left == right,
    }
}

const SHA256_K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// Computes a lowercase hexadecimal SHA-256 digest without extra crate dependencies.
fn sha256_hex(data: &[u8]) -> String {
    let mut state: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity((data.len() + 9).div_ceil(64) * 64);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.as_chunks::<64>().0 {
        compress_sha256_block(&mut state, block);
    }

    let mut hex = String::with_capacity(64);
    for word in state {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{word:08x}");
    }
    hex
}

fn compress_sha256_block(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut schedule = [0_u32; 64];
    for (index, word) in block.as_chunks::<4>().0.iter().enumerate() {
        schedule[index] = u32::from_be_bytes(*word);
    }
    for index in 16..64 {
        let s0 = schedule[index - 15].rotate_right(7)
            ^ schedule[index - 15].rotate_right(18)
            ^ (schedule[index - 15] >> 3);
        let s1 = schedule[index - 2].rotate_right(17)
            ^ schedule[index - 2].rotate_right(19)
            ^ (schedule[index - 2] >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(s1);
    }

    let mut regs = *state;
    for index in 0..64 {
        let s1 = regs[4].rotate_right(6) ^ regs[4].rotate_right(11) ^ regs[4].rotate_right(25);
        let choose = (regs[4] & regs[5]) ^ ((!regs[4]) & regs[6]);
        let temp1 = regs[7]
            .wrapping_add(s1)
            .wrapping_add(choose)
            .wrapping_add(SHA256_K[index])
            .wrapping_add(schedule[index]);
        let s0 = regs[0].rotate_right(2) ^ regs[0].rotate_right(13) ^ regs[0].rotate_right(22);
        let majority = (regs[0] & regs[1]) ^ (regs[0] & regs[2]) ^ (regs[1] & regs[2]);
        let temp2 = s0.wrapping_add(majority);

        regs[7] = regs[6];
        regs[6] = regs[5];
        regs[5] = regs[4];
        regs[4] = regs[3].wrapping_add(temp1);
        regs[3] = regs[2];
        regs[2] = regs[1];
        regs[1] = regs[0];
        regs[0] = temp1.wrapping_add(temp2);
    }

    for (slot, delta) in state.iter_mut().zip(regs) {
        *slot = slot.wrapping_add(delta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_workspace(name: &str) -> Result<PathBuf, String> {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "genshin-go-plugin-test-{}-{}-{id}",
            std::process::id(),
            name
        ));
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(dir)
    }

    fn required() -> Vec<String> {
        vec![
            "FufuLauncher.UnlockerIsland.dll".into(),
            "config.ini".into(),
        ]
    }

    #[test]
    fn sha256_matches_standard_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn installs_updates_verifies_after_restart_and_uninstalls_managed_files_only()
    -> Result<(), String> {
        let root = temp_workspace("lifecycle")?;
        let source = root.join("source");
        let plugins_root = root.join("plugins");
        fs::create_dir_all(&source).map_err(|e| e.to_string())?;
        fs::write(source.join("FufuLauncher.UnlockerIsland.dll"), b"dll-v1")
            .map_err(|e| e.to_string())?;
        fs::write(source.join("config.ini"), b"fps=120").map_err(|e| e.to_string())?;
        // Runtime artifacts in the source folder must never be copied into managed storage.
        fs::write(source.join("heartbeat.log"), b"log").map_err(|e| e.to_string())?;
        fs::write(source.join("uids.json"), b"{}").map_err(|e| e.to_string())?;

        let files = required();
        assert_eq!(
            inspect(&plugins_root, "fufu-unlocker-island", "fufu", &files)?,
            InstallStatus::NotInstalled
        );

        let installed = install(
            &plugins_root,
            "fufu-unlocker-island",
            "fufu",
            "1830f9d7",
            &source,
            &files,
        )?;
        let managed = plugins_root.join("fufu-unlocker-island");
        assert!(managed.join("FufuLauncher.UnlockerIsland.dll").is_file());
        assert!(managed.join("config.ini").is_file());
        assert!(!managed.join("heartbeat.log").exists());
        assert!(!managed.join("uids.json").exists());

        // Simulates reopening the launcher and verifying persisted state from disk.
        assert_eq!(
            inspect(&plugins_root, "fufu-unlocker-island", "fufu", &files)?,
            InstallStatus::Installed(installed.clone())
        );

        // Same content reports up to date.
        assert_eq!(
            update(
                &plugins_root,
                "fufu-unlocker-island",
                "fufu",
                "1830f9d7",
                &source,
                &files,
            )?,
            UpdateOutcome::UpToDate(installed.clone())
        );

        // Changing a source file triggers an actual update on user request.
        fs::write(source.join("FufuLauncher.UnlockerIsland.dll"), b"dll-v2")
            .map_err(|e| e.to_string())?;
        let updated = match update(
            &plugins_root,
            "fufu-unlocker-island",
            "fufu",
            "1830f9d7",
            &source,
            &files,
        )? {
            UpdateOutcome::Updated(record) => record,
            UpdateOutcome::UpToDate(_) => return Err("expected updated payload".into()),
        };
        assert_ne!(updated.content_sha256, installed.content_sha256);
        assert_eq!(
            fs::read(managed.join("FufuLauncher.UnlockerIsland.dll")).map_err(|e| e.to_string())?,
            b"dll-v2"
        );

        // Uninstall removes only managed files and preserves source files.
        uninstall(&plugins_root, "fufu-unlocker-island", "fufu", &files)?;
        assert!(!managed.exists());
        assert!(source.join("FufuLauncher.UnlockerIsland.dll").is_file());
        assert_eq!(
            inspect(&plugins_root, "fufu-unlocker-island", "fufu", &files)?,
            InstallStatus::NotInstalled
        );

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn failed_update_preserves_existing_installation_and_missing_file_is_detected()
    -> Result<(), String> {
        let root = temp_workspace("failure")?;
        let source = root.join("source");
        let broken_source = root.join("broken");
        let plugins_root = root.join("plugins");
        fs::create_dir_all(&source).map_err(|e| e.to_string())?;
        fs::create_dir_all(&broken_source).map_err(|e| e.to_string())?;
        fs::write(source.join("FufuLauncher.UnlockerIsland.dll"), b"dll-v1")
            .map_err(|e| e.to_string())?;
        fs::write(source.join("config.ini"), b"fps=120").map_err(|e| e.to_string())?;
        fs::write(broken_source.join("config.ini"), b"only-config").map_err(|e| e.to_string())?;

        let files = required();
        let installed = install(
            &plugins_root,
            "fufu-unlocker-island",
            "fufu",
            "1830f9d7",
            &source,
            &files,
        )?;

        let err = update(
            &plugins_root,
            "fufu-unlocker-island",
            "fufu",
            "1830f9d7",
            &broken_source,
            &files,
        );
        assert!(matches!(err, Err(msg) if msg.contains("缺少必需文件")));
        assert_eq!(
            inspect(&plugins_root, "fufu-unlocker-island", "fufu", &files)?,
            InstallStatus::Installed(installed.clone())
        );

        // Removing a managed file externally is detected on inspect.
        fs::remove_file(
            plugins_root
                .join("fufu-unlocker-island")
                .join("FufuLauncher.UnlockerIsland.dll"),
        )
        .map_err(|e| e.to_string())?;
        assert!(matches!(
            inspect(&plugins_root, "fufu-unlocker-island", "fufu", &files)?,
            InstallStatus::Corrupted { reason, .. }
                if reason == "缺少 FufuLauncher.UnlockerIsland.dll"
        ));

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn replaced_managed_file_is_detected_and_can_be_reinstalled() -> Result<(), String> {
        let root = temp_workspace("replaced")?;
        let source = root.join("source");
        let plugins_root = root.join("plugins");
        let files = required();
        let subpath = "fufu-unlocker-island";
        let plugin_id = "fufu";
        let version = "1830f9d7";
        fs::create_dir_all(&source).map_err(|error| error.to_string())?;
        fs::write(source.join(&files[0]), b"original module").map_err(|error| error.to_string())?;
        fs::write(source.join(&files[1]), b"original config").map_err(|error| error.to_string())?;
        let installed = install(&plugins_root, subpath, plugin_id, version, &source, &files)?;

        fs::write(
            plugins_root.join(subpath).join(&files[0]),
            b"replaced module",
        )
        .map_err(|error| error.to_string())?;
        assert!(matches!(
            inspect(&plugins_root, subpath, plugin_id, &files)?,
            InstallStatus::Corrupted { .. }
        ));
        assert!(update(&plugins_root, subpath, plugin_id, version, &source, &files).is_err());
        assert_eq!(
            install(&plugins_root, subpath, plugin_id, version, &source, &files)?,
            installed
        );
        assert_eq!(
            inspect(&plugins_root, subpath, plugin_id, &files)?,
            InstallStatus::Installed(installed)
        );

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    #[ignore = "requires local FufuLauncher install"]
    fn real_local_fufu_source_installs_and_uninstalls_cleanly() -> Result<(), String> {
        let source = Path::new(r"C:\softwares\FufuLauncher\Plugins\FuFuPlugin");
        if !source.is_dir() {
            return Ok(());
        }
        let root = temp_workspace("real-local-fufu")?;
        let plugins_root = root.join("plugins");
        let files = required();

        let installed = install(
            &plugins_root,
            "fufu-unlocker-island",
            "fufu-unlocker-island",
            "1830f9d7（2026-11-18 固定提交）",
            source,
            &files,
        )?;
        let managed = plugins_root.join("fufu-unlocker-island");
        assert!(managed.join("FufuLauncher.UnlockerIsland.dll").is_file());
        assert!(managed.join("config.ini").is_file());
        assert!(!managed.join("heartbeat.log").exists());
        assert!(!managed.join("uids.json").exists());
        assert_eq!(
            inspect(
                &plugins_root,
                "fufu-unlocker-island",
                "fufu-unlocker-island",
                &files
            )?,
            InstallStatus::Installed(installed.clone())
        );
        assert_eq!(
            update(
                &plugins_root,
                "fufu-unlocker-island",
                "fufu-unlocker-island",
                "1830f9d7（2026-11-18 固定提交）",
                source,
                &files,
            )?,
            UpdateOutcome::UpToDate(installed)
        );
        uninstall(
            &plugins_root,
            "fufu-unlocker-island",
            "fufu-unlocker-island",
            &files,
        )?;
        assert!(!managed.exists());
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
