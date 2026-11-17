use crate::launcher::{DirectoryChoice, LauncherEnvironment};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Provides native settings, account storage, and game launch operations.
pub struct NativeEnvironment {
    settings: Result<PathBuf, String>,
}

impl NativeEnvironment {
    /// Creates the native environment for the current user.
    pub fn new() -> Self {
        Self {
            settings: settings_path(),
        }
    }

    #[cfg(target_os = "windows")]
    fn accounts_path(&self) -> Result<PathBuf, String> {
        Ok(self
            .settings
            .as_ref()
            .map_err(Clone::clone)?
            .with_file_name("accounts-passport.dpapi"))
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
            .and_then(|_| file.sync_all())
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
fn validate_installation(path: &Path) -> Result<PathBuf, String> {
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
        let canonical = directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        // The shell and the UI use ordinary Windows paths, rather than the
        // extended-length prefix that Rust's canonicalize returns.
        #[cfg(target_os = "windows")]
        if let Some(path) = canonical.to_str() {
            if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
                return Ok(PathBuf::from(format!(r"\\{unc}")));
            }
            if let Some(path) = path.strip_prefix(r"\\?\") {
                return Ok(PathBuf::from(path));
            }
        }
        return Ok(canonical);
    }
    Err(
        "未找到有效国服官服安装，请选择包含 YuanShen.exe、YuanShen_Data 和 config.ini 的目录"
            .into(),
    )
}

impl LauncherEnvironment for NativeEnvironment {
    fn check_authorization(&self, account_id: String) -> crate::accounts::AccountOperation<bool> {
        #[cfg(target_os = "windows")]
        {
            let path = self.accounts_path();
            account_work(move || {
                let authorization = crate::account_store::load(&path?)?
                    .into_iter()
                    .find(|a| a.account_id == account_id)
                    .ok_or("账号授权不存在，请重新扫码")?;
                crate::account_service::check(authorization)
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
            account_work(move || crate::account_service::poll(qr))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = qr;
            Box::pin(async { Err("本期仅支持 Windows 扫码授权".into()) })
        }
    }
    fn load_accounts(&self) -> Result<Vec<String>, String> {
        #[cfg(target_os = "windows")]
        {
            Ok(crate::account_store::load(&self.accounts_path()?)?
                .into_iter()
                .map(|a| a.account_id)
                .collect())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(Vec::new())
        }
    }
    fn save_authorization(
        &self,
        authorization: crate::accounts::Authorization,
    ) -> Result<Vec<String>, String> {
        #[cfg(target_os = "windows")]
        {
            crate::account_store::save(&self.accounts_path()?, authorization)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = authorization;
            Err("本期仅支持 Windows 凭据保护".into())
        }
    }
    fn create_qr(&self) -> crate::accounts::AccountOperation<crate::accounts::QrSession> {
        #[cfg(target_os = "windows")]
        {
            account_work(crate::account_service::create)
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
        let bytes = serde_json::to_vec_pretty(&Settings {
            version: 1,
            default_directory: directory.into(),
        })
        .map_err(|error| error.to_string())?;
        atomic_write(destination, &bytes)
    }

    fn validate(&self, directory: &Path) -> Result<PathBuf, String> {
        validate_installation(directory)
    }

    fn launch(&self, directory: &Path) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            windows_environment::launch(&validate_installation(directory)?)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = directory;
            Err("本期仅支持 Windows 启动游戏".into())
        }
    }

    fn choose_directory(&self) -> DirectoryChoice {
        Box::pin(async {
            nsh_core::element::pick_paths(
                nsh_core::file_dialog::OpenDialog::folders().title("选择国服官服游戏目录"),
            )
            .await
            .map(|paths| paths.and_then(|paths| paths.into_iter().next()))
        })
    }
}

#[cfg(target_os = "windows")]
mod windows_environment {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::{
            Foundation::ERROR_SUCCESS,
            Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
            System::Registry::*,
            UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        },
        core::{PCWSTR, PWSTR},
    };

    fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }

    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }

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
                &mut handle,
            )
        } == ERROR_SUCCESS)
            .then(|| Key(handle))
    }

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
                Some(&mut bytes),
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
                Some(&mut bytes),
            ) != ERROR_SUCCESS
            {
                return None;
            }
            let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            String::from_utf16(&buffer[..end]).ok()
        }
    }

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
                    let mut length = name.len() as u32;
                    // Registry key names have at most 255 UTF-16 code units.
                    let status = unsafe {
                        RegEnumKeyExW(
                            uninstall.0,
                            index,
                            Some(PWSTR(name.as_mut_ptr())),
                            &mut length,
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

    pub fn launch(directory: &Path) -> Result<(), String> {
        let executable = wide(directory.join("YuanShen.exe").as_os_str());
        let directory = wide(directory.as_os_str());
        let operation = wide(std::ffi::OsStr::new("open"));
        // Shell execution respects the game's elevation manifest; no shell command string or arguments are constructed.
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(executable.as_ptr()),
                PCWSTR::null(),
                PCWSTR(directory.as_ptr()),
                SW_SHOWNORMAL,
            )
        };
        let code = result.0 as isize;
        if code > 32 {
            Ok(())
        } else {
            Err(format!(
                "Windows 未能启动游戏（错误码 {code}），请检查权限或在系统提示中允许启动"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox() -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/t01-fixtures")
            .join(format!("{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn installation(root: &Path) -> PathBuf {
        let path = root.join("原神 Game");
        fs::create_dir_all(path.join("YuanShen_Data")).unwrap();
        fs::write(path.join("YuanShen.exe"), b"fixture: never executed").unwrap();
        fs::write(
            path.join("config.ini"),
            "[General]\ngame_version=6.0.0\nchannel=1\nsub_channel=1\ngame_biz=hk4e_cn\n",
        )
        .unwrap();
        path
    }

    #[test]
    fn native_environment_accepts_a_legacy_launcher_and_rejects_a_channel_installation() {
        let root = sandbox();
        let game = installation(&root);
        fs::write(
            root.join("config.ini"),
            "[launcher]\ngame_install_path=原神 Game\n",
        )
        .unwrap();
        let environment = NativeEnvironment {
            settings: Ok(root.join("settings.json")),
        };
        let expected = environment.validate(&game).unwrap();
        assert_eq!(environment.validate(&root).unwrap(), expected);
        fs::write(
            game.join("config.ini"),
            "[General]\ngame_version=6.0.0\nchannel=14\nsub_channel=0\n",
        )
        .unwrap();
        assert!(environment.validate(&game).is_err());
    }

    #[test]
    fn saved_default_survives_a_new_native_environment_and_can_be_replaced() {
        let root = sandbox();
        let settings = root.join("config/settings.json");
        let first = NativeEnvironment {
            settings: Ok(settings.clone()),
        };
        assert_eq!(first.load_default().unwrap(), None);
        let game = installation(&root);
        let game = first.validate(&game).unwrap();
        first.save_default(&game).unwrap();
        let reopened = NativeEnvironment {
            settings: Ok(settings.clone()),
        };
        assert_eq!(reopened.load_default().unwrap(), Some(game));
        let other = root.join("another installation");
        reopened.save_default(&other).unwrap();
        assert_eq!(first.load_default().unwrap(), Some(other));
        fs::write(settings, b"corrupt json").unwrap();
        assert!(reopened.load_default().is_err());
    }

    #[test]
    fn native_environment_rejects_missing_game_files() {
        let root = sandbox();
        let game = installation(&root);
        let environment = NativeEnvironment {
            settings: Ok(root.join("settings.json")),
        };
        fs::remove_file(game.join("YuanShen.exe")).unwrap();
        assert!(environment.validate(&game).is_err());
    }
}
