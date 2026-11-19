# Genshin-Go

## Windows account authorization

The launcher uses the [UIGF Rust SDK](https://github.com/UIGF-org/mihoyo-api-sdk) for Passport App QR login and session verification. After confirmation it exchanges the SToken for account cookies and checks that the account has a unique official mainland-China Genshin UID before storing the authorization.

Use **扫码添加米哈游账号** to create a new QR code and confirm it in 米游社. The UI shows scanning, confirmation, expiry, cancellation and network errors. Expired codes can be refreshed. The launcher stores one authorization in `%LOCALAPPDATA%/Genshin-Go/launcher.sqlite3`, with the Cookie encrypted using current-user Windows DPAPI. Reopening restores the account and its verified UID. Use **检查授权** to check the saved account or **重新扫码** to renew it with the same account and UID. Sign in to the game in the official client after clicking **普通启动**. Authorization is not portable between Windows users or computers.

In **UID 数据空间**, enter a game UID and choose **创建 UID 数据空间**. You can create and switch spaces without signing in. The launcher restores the selected space on reopening. Scan authorization associates only the game UID verified by 米游社, while keeping any existing space selection. See the [T04 validation record](docs/validation/t04.md).

The launcher assigns one device ID to the local database and reuses it for refreshed QR codes, new scans and authorization requests after reopening.

Upgrading from the multi-account version removes its saved authorizations. You can switch between existing UID data spaces before scanning again. To verify the upgrade with isolated Windows data, run `./docs/validation/issue-32-windows.ps1` in PowerShell 7.2 or later. Add `-LiveScan` to verify a real scan and two restarts. The script removes its temporary directory after a successful run. Use `-KeepData` to retain it for inspection. A failed run leaves the directory in place for diagnosis.

Use **移除账号** to delete the saved 米游社 authorization and keep the UID spaces and their records. Scanning again verifies the account's game UID and links it to the existing space. To clear local records, open **删除历史记录**, select a UID, and confirm the UID displayed. This keeps the UID space and any saved authorization available.

## Windows community sign-in

The sole saved 米哈游账号 is the sign-in account. The launcher shows **社区签到** and checks for the day's unfinished community sign-in when opened, at Beijing midnight, and after Windows resumes. Switch UID data spaces independently. The launcher only runs checks while it is open.

The **社区签到奖励日历** shows today's result and results checked during the current session in the past six Beijing dates. It displays the reward amount from the 米游社 task definition when 米游社 confirms delivery. If sign-in succeeds and delivery is still unconfirmed, use **检查奖励领取** to refresh the reward status.

Use **重试社区签到** in the calendar after a failure. If 米游社 requires verification, complete verification or sign-in in the app, then retry. If the launcher asks you to scan again, use **重新扫码** with the same account. Older authorizations may need a fresh scan to supply the separately encrypted community sign-in session. See the [T11 sign-in validation](docs/validation/t11.md) and [T13 calendar validation](docs/validation/t13.md).

## Windows launcher: existing official mainland-China installation

The Rust root discovers existing installations from the current-user and machine
uninstall registry entries (both registry views), legacy launcher `config.ini`
paths, and common Program Files locations. Discovery is bounded; installations
elsewhere can be selected with **选择游戏目录** or entered in the path field.
Select a discovered installation to save it as the default. The selected game
directory must contain `YuanShen.exe`, `YuanShen_Data`, and an official-channel
`config.ini`; choosing a legacy launcher directory also resolves its game path.

The default is stored atomically in `%LOCALAPPDATA%/Genshin-Go/settings.json`.
The app validates it again on reopening and before **普通启动**. Ordinary launch
uses the Windows shell with the game directory as the working directory; Windows
may ask for elevation according to the game's manifest. **已请求启动游戏** means
Windows accepted the request. The game opens its login screen separately.

## Windows launcher: download and install

Choose **选择安装位置** to select the parent for a new `Genshin Impact Game`
directory, then click **开始下载**. The launcher downloads the current mainland-China
official release and the Chinese voice pack. The selected drive needs room for the
installed files (about 150 GB for version 7.2.0). Choose a writable parent without
an existing `Genshin Impact Game` directory.

Use **暂停下载** and **继续下载** to control the transfer. Closing the launcher keeps
the verified data. On reopening, click **继续下载** to resume the saved task. Once
the install finishes, the new game directory becomes the default for **普通启动**.
See the [T06 protocol and Windows validation record](docs/validation/t06.md).

The isolated interaction suite runs the same Rust launcher with a controlled
application environment. It never launches the real game or writes the user's
settings:

On Windows, run the local checks in PowerShell 7.2 or later:

```powershell
./scripts/check-windows.ps1
```

The script runs formatting, Clippy, the offline ticket synchronization tests,
and the NSH checks and tests for both the root project and the interaction fixture.
Run the [live account validation](docs/validation/t02.md#重跑验证) separately.

Windows storage tests create their own temporary databases and use DPAPI under
the current Windows user. If a sandboxed test reports `Windows 无法保护或解密凭据`,
rerun the same command from that user's PowerShell session with access to DPAPI.
To run the isolated storage tests alone, use
`cargo test --locked --lib account_store::tests::`.

The fixture has its own NSH entry and must run from its directory. Its hidden
directory keeps the root runner from discovering tests for a different Rust root.
Both `.nsh/`
and `target/` remain generated directories. Windows launch validation steps are
recorded separately in `docs/validation/t01-windows-launch.js`; running them
requests a real game launch and changes the launcher's default directory.

An app written in Rust: its UI is `src/lib.rs`, and `nsh.json` names the root component (`"root": "App"`).

```sh
nsh run                      # this machine's backend; --target ios, android, web, macos, linux or windows
nsh check                    # cargo check of the crate, then type checks App.test.tsx
nsh test                     # cargo test, then App.test.tsx: the root tested from TypeScript, headless; --target to run it on a platform
nsh test nsh.json --trace    # headless: what the root mounts, and the time of each commit
nsh package --target web     # or ios, android, macos, linux, windows
```

`nsh run` and `nsh package` mount the root on every platform, the browser included, without QuickJS: the app is Rust
and native code from start to end (in the browser, with NSH's page script). Rust edits rebuild and restart the app.

```sh
cargo run
```

This starts the root directly on the machine's native desktop backend: GTK 4 on Linux, AppKit on macOS, Win32 on
Windows. Install the backend's development libraries before building.

The `nsh` commands build the app's runners from the NSH crates `Cargo.toml` depends on: crates.io's at the version of
the `nsh` you run (keep the `nsh-*` dependencies at that version when you upgrade `nsh`), or an NSH checkout's by path.

## Adding a TSX entry

TSX screens can use the Rust components and modules, with hot reloading for the TSX. The crate's set,
`#[native_module("genshin-go/native")]` in `src/lib.rs`, already exports `App`. Add an `App.tsx`:

```tsx
import { mount } from "nsh";
import { App } from "genshin-go/native";

mount(App);
```

then run it with `nsh run App.tsx` (`tsconfig.json` is there for it and the test): a named TSX entry
runs on QuickJS, while `nsh run` alone keeps running the Rust root. The first `nsh build App.tsx` writes the TypeScript
declarations of `genshin-go/native` into `.nsh/types`. Register more with `registry.register::<…>()` (components),
`registry.module::<…>()` and `registry.class::<…>()`.

NSH-generated files stay in `.nsh/`: runner sources in `.nsh/rust/` and default packages in `.nsh/out/`.
Cargo keeps its normal `target/` directory and respects your existing `CARGO_TARGET_DIR`.

## UI prototype

Open the throwaway launcher layout prototype on Windows with one command:

```powershell
Start-Process .\src\ui-prototype.html
```

Use the floating arrows or the left and right arrow keys to compare variants A, B and C. The URL's `?variant=` value keeps the selected layout on reload. Account, directory, sign-in and launch controls are local demonstrations; the production Rust UI remains the app entry.
