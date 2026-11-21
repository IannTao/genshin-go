use crate::launcher::{DirectoryChoice, LauncherEnvironment};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(target_os = "windows")]
mod process;

/// Provides native settings, account storage, and game launch operations.
pub struct NativeEnvironment {
    settings: Result<PathBuf, String>,
}

impl NativeEnvironment {
    /// Uses an explicit directory for the launcher's local application data.
    pub(crate) fn in_directory(directory: &Path) -> Self {
        Self {
            settings: Ok(directory.join("settings.json")),
        }
    }

    /// Creates the native environment for the current user.
    pub fn new() -> Self {
        Self {
            settings: settings_path(),
        }
    }

    #[cfg(target_os = "windows")]
    fn database_path(&self) -> Result<PathBuf, String> {
        Ok(self
            .settings
            .as_ref()
            .map_err(Clone::clone)?
            .with_file_name("launcher.sqlite3"))
    }
}

#[cfg(target_os = "windows")]
fn account_work<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> crate::accounts::AccountOperation<T> {
    Box::pin(async move {
        tokio::task::spawn_blocking(work)
            .await
            .map_err(|_| "账号操作未能完成，请重试".to_owned())?
    })
}

#[derive(Serialize, Deserialize)]
struct Settings {
    version: u32,
    default_directory: PathBuf,
}

fn settings_path() -> Result<PathBuf, String> {
    if let Some(directory) = std::env::var_os("GENSHIN_GO_DATA_DIR") {
        let directory = PathBuf::from(directory);
        if !directory.is_absolute() {
            return Err("GENSHIN_GO_DATA_DIR 必须是绝对路径".into());
        }
        return Ok(directory.join("settings.json"));
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|root| PathBuf::from(root).join("Genshin-Go").join("settings.json"))
        .ok_or_else(|| "无法确定本机配置目录".into())
}

/// Writes bytes through a temporary file and atomically replaces the destination.
pub(crate) fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = destination.parent().ok_or("配置路径无效")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let temporary = parent.join(format!("settings-{}-{unique}.tmp", std::process::id()));
    let result = (|| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| error.to_string())?;
        drop(file);
        #[cfg(target_os = "windows")]
        windows_environment::replace_settings(&temporary, destination)?;
        #[cfg(not(target_os = "windows"))]
        fs::rename(&temporary, destination).map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn ini_value<'a>(contents: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut current = "";
    for line in contents
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
    {
        if line.starts_with('[') && line.ends_with(']') {
            current = &line[1..line.len() - 1];
        } else if current.eq_ignore_ascii_case(section)
            && let Some((name, value)) = line.split_once('=')
            && name.trim().eq_ignore_ascii_case(key)
        {
            return Some(value.trim());
        }
    }
    None
}

/// Normalize a game directory or a legacy launcher's parent directory.
pub(crate) fn validate_installation(path: &Path) -> Result<PathBuf, String> {
    let mut candidates = vec![path.to_path_buf()];
    if let Ok(config) = fs::read_to_string(path.join("config.ini"))
        && let Some(value) = ini_value(&config, "launcher", "game_install_path")
    {
        let configured = PathBuf::from(value);
        candidates.push(if configured.is_absolute() {
            configured
        } else {
            path.join(configured)
        });
    }
    candidates.push(path.join("Genshin Impact Game"));
    for directory in candidates {
        let Ok(config) = fs::read_to_string(directory.join("config.ini")) else {
            continue;
        };
        if ini_value(&config, "General", "channel") != Some("1")
            || ini_value(&config, "General", "sub_channel") != Some("1")
            || ini_value(&config, "General", "game_version").is_none_or(str::is_empty)
            || ini_value(&config, "General", "game_biz").is_some_and(|biz| biz != "hk4e_cn")
            || !directory.join("YuanShen.exe").is_file()
            || !directory.join("YuanShen_Data").is_dir()
        {
            continue;
        }
        return normalize_directory(&directory);
    }
    Err(
        "未找到有效国服官服安装，请选择包含 YuanShen.exe、YuanShen_Data 和 config.ini 的目录"
            .into(),
    )
}

/// Returns the canonical directory path in the format used by the UI and shell.
pub(crate) fn normalize_directory(directory: &Path) -> Result<PathBuf, String> {
    let canonical = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    if let Some(path) = canonical.to_str() {
        if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{unc}")));
        }
        if let Some(path) = path.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(path));
        }
    }
    Ok(canonical)
}

/// Stores the game directory in the specified launcher settings file.
pub(crate) fn save_default_at(destination: &Path, directory: &Path) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(&Settings {
        version: 1,
        default_directory: directory.into(),
    })
    .map_err(|error| error.to_string())?;
    atomic_write(destination, &bytes)
}

impl LauncherEnvironment for NativeEnvironment {
    #[cfg(target_os = "windows")]
    fn uid_database_path(&self) -> Option<Result<PathBuf, String>> {
        Some(self.database_path())
    }

    #[cfg(target_os = "windows")]
    fn community_clock(&self) -> Option<crate::community_sign_in::Clock> {
        crate::community_service::clock()
    }

    #[cfg(target_os = "windows")]
    fn community_sign_in(
        &self,
        account: crate::profile::VerifiedAccount,
        day: u64,
        allow_submission: bool,
    ) -> crate::community_sign_in::Operation {
        use crate::community_sign_in::Failure;
        let path = self.database_path();
        Box::pin(async move {
            crate::community_service::sign_in(
                path.map_err(Failure::Other)?,
                account,
                day,
                allow_submission,
            )
            .await
        })
    }

    fn check_authorization(&self, account_id: String) -> crate::accounts::AccountOperation<bool> {
        #[cfg(target_os = "windows")]
        {
            let path = self.database_path();
            account_work(move || {
                let path = path?;
                let authorization = crate::account_store::load_authorization(&path, &account_id)?;
                crate::account_service::check(
                    &authorization,
                    &crate::account_store::device_id(&path)?,
                )
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = account_id;
            Box::pin(async { Err("本期仅支持 Windows 授权验证".into()) })
        }
    }
    fn poll_qr(
        &self,
        qr: crate::accounts::QrSession,
    ) -> crate::accounts::AccountOperation<crate::accounts::QrStatus> {
        #[cfg(target_os = "windows")]
        {
            account_work(move || crate::account_service::poll(&qr))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = qr;
            Box::pin(async { Err("本期仅支持 Windows 扫码授权".into()) })
        }
    }
    fn load_profile(&self) -> Result<crate::profile::Profile, String> {
        #[cfg(target_os = "windows")]
        {
            crate::account_store::load(&self.database_path()?)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(crate::profile::Profile::default())
        }
    }
    fn select_uid(&self, uid: &str, create: bool) -> Result<crate::profile::Profile, String> {
        #[cfg(target_os = "windows")]
        {
            crate::account_store::select_uid(&self.database_path()?, uid, create)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (uid, create);
            Err("本期仅支持在 Windows 保存 UID 数据空间".into())
        }
    }
    fn delete_uid_history(&self, uid: &str) -> Result<crate::profile::Profile, String> {
        #[cfg(target_os = "windows")]
        {
            crate::account_store::delete_uid_history(&self.database_path()?, uid)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = uid;
            Err("本期仅支持在 Windows 删除 UID 历史记录".into())
        }
    }
    fn verify_uid(
        &self,
        authorization: crate::accounts::Authorization,
    ) -> crate::accounts::AccountOperation<String> {
        #[cfg(target_os = "windows")]
        {
            let path = self.database_path();
            account_work(move || {
                crate::account_service::verify_uid(
                    &authorization,
                    &crate::account_store::device_id(&path?)?,
                )
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = authorization;
            Box::pin(async { Err("本期仅支持 Windows 账号归属验证".into()) })
        }
    }
    fn save_authorization(
        &self,
        authorization: crate::accounts::Authorization,
        uid: &str,
        expected: Option<&crate::profile::VerifiedAccount>,
    ) -> Result<crate::profile::Profile, String> {
        #[cfg(target_os = "windows")]
        {
            crate::account_store::save(&self.database_path()?, authorization, uid, expected)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (authorization, uid, expected);
            Err("本期仅支持 Windows 凭据保护".into())
        }
    }
    fn remove_authorization(
        &self,
        account: &crate::profile::VerifiedAccount,
    ) -> Result<crate::profile::Profile, String> {
        #[cfg(target_os = "windows")]
        {
            crate::account_store::remove_authorization(&self.database_path()?, account)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = account;
            Err("本期仅支持在 Windows 移除米游社授权".into())
        }
    }
    fn create_qr(&self) -> crate::accounts::AccountOperation<crate::accounts::QrSession> {
        #[cfg(target_os = "windows")]
        {
            let path = self.database_path();
            account_work(move || {
                crate::account_service::create(&crate::account_store::device_id(&path?)?)
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            Box::pin(async { Err("本期仅支持 Windows 扫码授权".into()) })
        }
    }
    fn discover(&self) -> Result<Vec<PathBuf>, String> {
        #[cfg(target_os = "windows")]
        {
            let mut candidates = windows_environment::registry_directories();
            for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Some(root) = std::env::var_os(variable) {
                    for relative in [
                        "Genshin Impact",
                        "miHoYo Launcher/games/Genshin Impact Game",
                        "HoYoPlay/games/Genshin Impact Game",
                    ] {
                        candidates.push(PathBuf::from(&root).join(relative));
                    }
                }
            }
            let mut valid = Vec::new();
            for candidate in candidates {
                if let Ok(path) = validate_installation(&candidate)
                    && !valid.contains(&path)
                {
                    valid.push(path);
                }
            }
            Ok(valid)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(Vec::new())
        }
    }

    fn load_default(&self) -> Result<Option<PathBuf>, String> {
        let path = self.settings.as_ref().map_err(Clone::clone)?;
        let contents = match fs::read(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let settings: Settings =
            serde_json::from_slice(&contents).map_err(|error| error.to_string())?;
        if settings.version != 1 {
            return Err("无法读取此版本的配置".into());
        }
        Ok(Some(settings.default_directory))
    }

    fn save_default(&self, directory: &Path) -> Result<(), String> {
        let destination = self.settings.as_ref().map_err(Clone::clone)?;
        save_default_at(destination, directory)
    }

    fn validate(&self, directory: &Path) -> Result<PathBuf, String> {
        validate_installation(directory)
    }

    fn launch(&self, directory: &Path) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            self.launch_with_process(directory).map(drop)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = directory;
            Err("本期仅支持 Windows 启动游戏".into())
        }
    }

    #[cfg(target_os = "windows")]
    fn launch_with_process(
        &self,
        directory: &Path,
    ) -> Result<Option<crate::launcher::GameProcess>, String> {
        process::launch_game(&validate_installation(directory)?)
    }

    #[cfg(target_os = "windows")]
    fn start_scanner(&self, program: &Path) -> crate::launcher::ScannerLaunch {
        let program = program.to_path_buf();
        Box::pin(process::start_scanner(program))
    }

    fn choose_directory(&self) -> DirectoryChoice {
        pick_directory("选择国服官服游戏目录")
    }

    fn choose_install_parent(&self) -> DirectoryChoice {
        pick_directory("选择存放新安装的上一级目录")
    }

    #[cfg(target_os = "windows")]
    fn supports_install(&self) -> bool {
        true
    }

    #[cfg(target_os = "windows")]
    fn pending_install(&self) -> Result<Option<PathBuf>, String> {
        crate::game_install::windows::pending(self.settings.as_ref().map_err(Clone::clone)?)
    }

    #[cfg(target_os = "windows")]
    fn install_game(
        &self,
        parent: Option<PathBuf>,
        control: crate::game_install::InstallControl,
    ) -> crate::game_install::InstallOperation {
        let settings = self.settings.clone();
        Box::pin(async move {
            let settings = settings?;
            tokio::task::spawn_blocking(move || {
                crate::game_install::windows::install(
                    &settings,
                    parent.as_deref(),
                    &control,
                    |dir| save_default_at(&settings, dir),
                )
            })
            .await
            .map_err(|error| format!("安装任务意外停止：{error}"))?
        })
    }
}

fn pick_directory(title: &'static str) -> DirectoryChoice {
    Box::pin(async move {
        nsh_core::element::pick_paths(nsh_core::file_dialog::OpenDialog::folders().title(title))
            .await
            .map(|paths| paths.and_then(|paths| paths.into_iter().next()))
    })
}

#[cfg(target_os = "windows")]
mod windows_environment {
    use super::{Path, PathBuf};
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::{
            Foundation::ERROR_SUCCESS,
            Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
            System::Registry::{
                HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY,
                KEY_WOW64_64KEY, REG_SAM_FLAGS, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW,
                RegGetValueW, RegOpenKeyExW,
            },
        },
        core::{PCWSTR, PWSTR},
    };

    fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }

    struct Key(HKEY);
    impl Drop for Key {
        #[expect(unsafe_code, reason = "Releases the registry handle owned by this key")]
        fn drop(&mut self) {
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }

    #[expect(
        unsafe_code,
        reason = "Opens a registry key using live UTF-16 storage and an output handle"
    )]
    fn open(root: HKEY, path: &str, view: REG_SAM_FLAGS) -> Option<Key> {
        let name = wide(std::ffi::OsStr::new(path));
        let mut handle = HKEY::default();
        // The name is NUL-terminated and the output handle is owned by Key.
        (unsafe {
            RegOpenKeyExW(
                root,
                PCWSTR(name.as_ptr()),
                None,
                KEY_READ | view,
                &raw mut handle,
            )
        } == ERROR_SUCCESS)
            .then(|| Key(handle))
    }

    #[expect(unsafe_code, reason = "Reads registry data into a sized UTF-16 buffer")]
    fn value(key: &Key, name: &str) -> Option<String> {
        let name = wide(std::ffi::OsStr::new(name));
        let mut bytes = 0;
        // Query size before allocating, and accept string values only.
        unsafe {
            if RegGetValueW(
                key.0,
                PCWSTR::null(),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&raw mut bytes),
            ) != ERROR_SUCCESS
                || bytes > 65536
            {
                return None;
            }
            let mut buffer = vec![0_u16; bytes as usize / 2 + 1];
            if RegGetValueW(
                key.0,
                PCWSTR::null(),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&raw mut bytes),
            ) != ERROR_SUCCESS
            {
                return None;
            }
            let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            String::from_utf16(&buffer[..end]).ok()
        }
    }

    #[expect(
        unsafe_code,
        reason = "Enumerates registry names into a fixed Windows key-name buffer"
    )]
    pub fn registry_directories() -> Vec<PathBuf> {
        let mut result = Vec::new();
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
                let Some(uninstall) = open(
                    root,
                    "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
                    view,
                ) else {
                    continue;
                };
                for index in 0..10000 {
                    let mut name = [0_u16; 256];
                    let mut length = 256_u32;
                    // Registry key names have at most 255 UTF-16 code units.
                    let status = unsafe {
                        RegEnumKeyExW(
                            uninstall.0,
                            index,
                            Some(PWSTR(name.as_mut_ptr())),
                            &raw mut length,
                            None,
                            None,
                            None,
                            None,
                        )
                    };
                    if status != ERROR_SUCCESS {
                        break;
                    }
                    let name = String::from_utf16_lossy(&name[..length as usize]);
                    let Some(key) = open(uninstall.0, &name, view) else {
                        continue;
                    };
                    if !name.starts_with("hk4e_cn")
                        && value(&key, "GameBiz").as_deref() != Some("hk4e_cn")
                        && value(&key, "DisplayName").as_deref() != Some("原神")
                    {
                        continue;
                    }
                    for field in ["InstallLocation", "InstallPath"] {
                        if let Some(path) = value(&key, field) {
                            result.push(path.into());
                        }
                    }
                    if let Some(icon) = value(&key, "DisplayIcon") {
                        let path = icon
                            .rsplit_once(',')
                            .filter(|(_, suffix)| suffix.trim().parse::<i32>().is_ok())
                            .map_or(icon.as_str(), |(path, _)| path)
                            .trim()
                            .trim_matches('"');
                        if let Some(parent) = Path::new(path).parent() {
                            result.push(parent.into());
                        }
                    }
                }
            }
        }
        result
    }

    #[expect(
        unsafe_code,
        reason = "Windows atomically replaces the file using two live UTF-16 paths"
    )]
    pub fn replace_settings(from: &Path, to: &Path) -> Result<(), String> {
        let from = wide(from.as_os_str());
        let to = wide(to.as_os_str());
        // Both terminated paths stay alive for this atomic replacement call.
        unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/t01-fixtures")
            .join(format!("{}-{unique}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn installation(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let path = root.join("原神 Game");
        fs::create_dir_all(path.join("YuanShen_Data"))?;
        fs::write(path.join("YuanShen.exe"), b"fixture: never executed")?;
        fs::write(
            path.join("config.ini"),
            "[General]\ngame_version=6.0.0\nchannel=1\nsub_channel=1\ngame_biz=hk4e_cn\n",
        )?;
        Ok(path)
    }

    #[test]
    fn native_environment_accepts_a_legacy_launcher_and_rejects_a_channel_installation()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = sandbox()?;
        let game = installation(&root)?;
        fs::write(
            root.join("config.ini"),
            "[launcher]\ngame_install_path=原神 Game\n",
        )?;
        let environment = NativeEnvironment {
            settings: Ok(root.join("settings.json")),
        };
        let expected = environment.validate(&game)?;
        assert_eq!(environment.validate(&root)?, expected);
        fs::write(
            game.join("config.ini"),
            "[General]\ngame_version=6.0.0\nchannel=14\nsub_channel=0\n",
        )?;
        assert!(environment.validate(&game).is_err());
        Ok(())
    }

    #[test]
    fn saved_default_survives_a_new_native_environment_and_can_be_replaced()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = sandbox()?;
        let settings = root.join("config/settings.json");
        let first = NativeEnvironment {
            settings: Ok(settings.clone()),
        };
        assert_eq!(first.load_default()?, None);
        let game = installation(&root)?;
        let game = first.validate(&game)?;
        first.save_default(&game)?;
        let reopened = NativeEnvironment {
            settings: Ok(settings.clone()),
        };
        assert_eq!(reopened.load_default()?, Some(game));
        let other = root.join("another installation");
        reopened.save_default(&other)?;
        assert_eq!(first.load_default()?, Some(other));
        fs::write(settings, b"corrupt json")?;
        assert!(reopened.load_default().is_err());
        Ok(())
    }

    #[test]
    fn native_environment_rejects_missing_game_files() -> Result<(), Box<dyn std::error::Error>> {
        let root = sandbox()?;
        let game = installation(&root)?;
        let environment = NativeEnvironment {
            settings: Ok(root.join("settings.json")),
        };
        fs::remove_file(game.join("YuanShen.exe"))?;
        assert!(environment.validate(&game).is_err());
        Ok(())
    }
}
