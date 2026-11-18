//! Current-user DPAPI storage. Plaintext never enters a temporary or final file.
use crate::accounts::Authorization;
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Deserialize)]
struct StoredAccounts {
    version: u32,
    accounts: Vec<Authorization>,
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

/// Loads the current user's DPAPI-protected account authorizations.
pub fn load(path: &Path) -> Result<Vec<Authorization>, String> {
    let mut bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("无法读取本机授权文件".into()),
    };
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("授权文件过大，无法读取".into());
    }
    let mut plaintext = crypt(&mut bytes, false)?;
    let parsed = serde_json::from_slice::<StoredAccounts>(&plaintext);
    plaintext.fill(0);
    let stored = parsed.map_err(|_| "授权文件损坏，未覆盖已有数据")?;
    if stored.version != 2 {
        return Err("不支持此版本的授权文件".into());
    }
    for authorization in &stored.accounts {
        authorization.validate()?;
    }
    Ok(stored.accounts)
}

/// Atomically saves or replaces one DPAPI-protected authorization.
pub fn save(path: &Path, authorization: Authorization) -> Result<Vec<String>, String> {
    authorization.validate()?;
    let directory = path.parent().ok_or("授权目录无效")?;
    fs::create_dir_all(directory).map_err(|_| "无法创建本机授权目录")?;
    // Serialize read/modify/write across launcher processes, with OS-managed unlock on close.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("accounts.lock"))
        .map_err(|_| "无法锁定授权文件")?;
    lock.try_lock()
        .map_err(|_| "另一个启动器正在保存账号，请重试")?;
    let mut accounts = load(path)?;
    accounts.retain(|a| a.account_id != authorization.account_id);
    accounts.push(authorization);
    let ids = accounts.iter().map(|a| a.account_id.clone()).collect();
    let mut plaintext = serde_json::to_vec(&StoredAccounts {
        version: 2,
        accounts,
    })
    .map_err(|_| "无法编码授权")?;
    let encrypted = crypt(&mut plaintext, true);
    plaintext.fill(0);
    crate::native_environment::atomic_write(path, &encrypted?)?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_protected_accounts_round_trip_replace_and_reject_corruption()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = std::env::temp_dir().join(format!(
            "genshin-go-accounts-{:?}",
            windows::core::GUID::new()?
        ));
        let path = directory.join("accounts.dpapi");
        let grant = |id: &str, token: &str| Authorization {
            account_id: id.into(),
            cookie_header: token.into(),
        };
        assert!(load(&path)?.is_empty());
        assert_eq!(save(&path, grant("10001", "test-secret-one"))?, ["10001"]);
        assert_eq!(
            save(&path, grant("10002", "test-secret-two"))?,
            ["10001", "10002"]
        );
        let encrypted = fs::read(&path)?;
        assert!(
            !encrypted
                .windows(b"test-secret".len())
                .any(|w| w == b"test-secret")
        );
        assert_eq!(load(&path)?[0].cookie_header, "test-secret-one");
        save(&path, grant("10001", "test-secret-renewed"))?;
        let reopened = load(&path)?;
        assert_eq!(reopened.len(), 2);
        assert_eq!(
            reopened
                .iter()
                .find(|a| a.account_id == "10001")
                .ok_or("renewed account missing")?
                .cookie_header,
            "test-secret-renewed"
        );
        fs::write(&path, b"corrupt")?;
        assert!(load(&path).is_err());
        assert!(save(&path, grant("10003", "new-secret")).is_err());
        assert_eq!(fs::read(&path)?, b"corrupt");
        fs::remove_file(&path)?;
        fs::remove_file(directory.join("accounts.lock"))?;
        fs::remove_dir(directory)?;
        Ok(())
    }

    fn load_saved_passport_authorizations() -> Result<Vec<Authorization>, String> {
        let path = std::path::PathBuf::from(
            std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA missing")?,
        )
        .join("Genshin-Go")
        .join("accounts-passport.dpapi");
        load(&path)
    }

    #[test]
    #[ignore = "reads the current user's real protected authorization"]
    fn restored_passport_authorization_has_paired_cookie_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let accounts = load_saved_passport_authorizations()?;
        assert!(!accounts.is_empty());
        assert!(
            accounts
                .iter()
                .any(|account| (account.cookie_header.contains("ltoken=")
                    && account.cookie_header.contains("cookie_token="))
                    || account.cookie_header.contains("ltoken_v2=")
                        && account.cookie_header.contains("ltmid_v2=")
                        && account.cookie_header.contains("cookie_token_v2="))
        );
        Ok(())
    }

    #[test]
    #[ignore = "uses the current user's saved authorization with the live passport service"]
    fn live_passport_accepts_saved_authorization() -> Result<(), Box<dyn std::error::Error>> {
        for account in load_saved_passport_authorizations()? {
            if crate::account_service::check(&account)? {
                return Ok(());
            }
        }
        Err("no saved authorization was accepted by the passport service".into())
    }
}
