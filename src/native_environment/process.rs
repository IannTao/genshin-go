//! Launches Windows game and scanner processes with scoped lifetime tracking.

use crate::launcher::{GameProcess, ScannerInstance};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    ffi::{OsStr, OsString},
    mem::size_of,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    sync::Mutex,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};
use windows::{
    Win32::{
        Foundation::{ERROR_CANCELLED, ERROR_NO_MORE_FILES, HANDLE, WAIT_OBJECT_0},
        System::{
            Com::{
                COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateGuid, CoInitializeEx,
                CoUninitialize,
            },
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            RemoteDesktop::ProcessIdToSessionId,
            SystemInformation::GetSystemDirectoryW,
            Threading::WaitForSingleObject,
        },
        UI::{
            Shell::{
                SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
                ShellExecuteExW,
            },
            WindowsAndMessaging::{SHOW_WINDOW_CMD, SW_HIDE, SW_SHOWNORMAL},
        },
    },
    core::PCWSTR,
};

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

#[expect(
    unsafe_code,
    reason = "Passes live shell strings and owns the returned process handle"
)]
fn execute(
    program: &Path,
    directory: &Path,
    verb: &str,
    parameters: Option<&str>,
    show: SHOW_WINDOW_CMD,
) -> windows::core::Result<Option<OwnedHandle>> {
    let file = wide(program.as_os_str());
    let directory = wide(directory.as_os_str());
    let verb = wide(OsStr::new(verb));
    let parameters = parameters.map(|value| wide(OsStr::new(value)));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).unwrap_or_default(),
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: parameters
            .as_ref()
            .map_or(PCWSTR::null(), |value| PCWSTR(value.as_ptr())),
        lpDirectory: PCWSTR(directory.as_ptr()),
        nShow: show.0,
        ..Default::default()
    };
    // Balance successful COM initializations, including S_FALSE on the UI thread.
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let result = ShellExecuteExW(&raw mut info);
        if initialized.is_ok() {
            CoUninitialize();
        }
        result?;
        Ok((!info.hProcess.is_invalid()).then(|| OwnedHandle::from_raw_handle(info.hProcess.0)))
    }
}

/// Launches the game with its usual shell verb and watches the returned process.
pub(super) fn launch_game(directory: &Path) -> Result<Option<GameProcess>, String> {
    execute(
        &directory.join("YuanShen.exe"),
        directory,
        "open",
        None,
        SW_SHOWNORMAL,
    )
    .map(|process| process.map(|handle| Box::new(move || running(&handle)) as GameProcess))
    .map_err(|error| format!("Windows 未能启动游戏（{error}），请检查权限或在系统提示中允许启动"))
}

#[expect(
    unsafe_code,
    reason = "Checks an owned Windows process handle without blocking"
)]
fn running(process: &OwnedHandle) -> bool {
    unsafe { WaitForSingleObject(HANDLE(process.as_raw_handle()), 0) != WAIT_OBJECT_0 }
}

#[expect(
    unsafe_code,
    reason = "Enumerates process names with an owned snapshot and an initialized output buffer"
)]
fn scanner_running() -> windows::core::Result<bool> {
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or_default(),
        ..Default::default()
    };
    unsafe {
        let mut current_session = 0;
        ProcessIdToSessionId(std::process::id(), &raw mut current_session)?;
        let snapshot =
            OwnedHandle::from_raw_handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?.0);
        let handle = HANDLE(snapshot.as_raw_handle());
        Process32FirstW(handle, &raw mut entry)?;
        loop {
            let end = entry
                .szExeFile
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(entry.szExeFile.len());
            if OsString::from_wide(&entry.szExeFile[..end]).eq_ignore_ascii_case("GGScanner.exe") {
                let mut session = 0;
                if ProcessIdToSessionId(entry.th32ProcessID, &raw mut session).is_ok()
                    && session == current_session
                {
                    return Ok(true);
                }
            }
            if let Err(error) = Process32NextW(handle, &raw mut entry) {
                return if error.code() == ERROR_NO_MORE_FILES.to_hresult() {
                    Ok(false)
                } else {
                    Err(error)
                };
            }
        }
    }
}

#[expect(
    unsafe_code,
    reason = "Reads the Windows system directory into a fixed-size UTF-16 buffer"
)]
fn powershell() -> Result<PathBuf, String> {
    let mut directory = [0_u16; 260];
    let length = unsafe { GetSystemDirectoryW(Some(&mut directory)) } as usize;
    if length == 0 || length >= directory.len() {
        return Err("无法确定 Windows 系统目录".into());
    }
    Ok(PathBuf::from(OsString::from_wide(&directory[..length]))
        .join(r"WindowsPowerShell\v1.0\powershell.exe"))
}

fn encoded(value: &OsStr) -> String {
    STANDARD.encode(
        value
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

#[expect(unsafe_code, reason = "Asks Windows for a fresh handshake identifier")]
fn handshake_token() -> Result<String, String> {
    unsafe { CoCreateGuid() }
        .map(|guid| format!("{guid:?}"))
        .map_err(|error| format!("无法准备扫描器关闭通道：{error}"))
}

fn elevate(program: &Path, port: u16, token: &str) -> Result<(), String> {
    static ELEVATION: Mutex<()> = Mutex::new(());
    let _elevation = ELEVATION
        .try_lock()
        .map_err(|_| "GGScanner 的权限请求尚未结束")?;
    if scanner_running().map_err(|error| format!("无法检查 GGScanner 进程：{error}"))? {
        return Err("GGScanner 已运行，请在其中启动原神管理服务并检查本机端口".into());
    }
    let shell = powershell()?;
    let directory = shell.parent().ok_or("Windows PowerShell 路径无效")?;
    // The file path is data encoded as Base64. The elevated script is fixed at build time.
    let script = format!(
        "$program64 = '{}'\n$port = {port}\n$token = '{token}'\n{}",
        encoded(program.as_os_str()),
        include_str!("scanner-process.ps1")
    );
    let parameters = format!(
        "-NoLogo -NoProfile -NonInteractive -EncodedCommand {}",
        encoded(OsStr::new(&script))
    );
    execute(&shell, directory, "runas", Some(&parameters), SW_HIDE)
        .map(drop)
        .map_err(|error| {
            if error.code() == ERROR_CANCELLED.to_hresult() {
                "已取消 GGScanner 管理员权限请求".into()
            } else {
                format!("无法启动 GGScanner（{error}）")
            }
        })
}

/// Opens an elevated scanner with a close channel tied to this launch's lifetime.
pub(super) async fn start_scanner(program: PathBuf) -> Result<ScannerInstance, String> {
    let channel_error = |error| format!("GGScanner 关闭通道不可用：{error}");
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(channel_error)?;
    let port = listener.local_addr().map_err(channel_error)?.port();
    let token = handshake_token()?;
    let launch_token = token.clone();
    let (result, launched) = oneshot::channel();
    // Windows can keep runas pending after the launcher stops waiting for approval.
    // This worker may outlive the async task. A dropped listener prevents a late start.
    std::thread::Builder::new()
        .name("ggscanner-elevation".into())
        .spawn(move || {
            let _ = result.send(elevate(&program, port, &launch_token));
        })
        .map_err(|error| format!("无法发起 GGScanner 权限请求：{error}"))?;
    launched
        .await
        .map_err(|_| "无法完成 GGScanner 权限请求")??;
    let (mut stream, _) = listener.accept().await.map_err(channel_error)?;
    stream
        .write_all(format!("{token}\n").as_bytes())
        .await
        .map_err(channel_error)?;
    if stream.read_u8().await.map_err(channel_error)? != 1 {
        return Err("无法打开选定的 GGScanner.exe，请检查程序和目录".into());
    }
    Ok(ScannerInstance::new(move || drop(stream)))
}
