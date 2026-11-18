# Genshin-Go

## Windows account authorization

Use **扫码添加米哈游账号** to create a new QR code and confirm it in 米游社. The UI shows scanning, confirmation, expiry, cancellation and network errors. Expired codes can be refreshed. Confirmed 米游社 Cookie authorizations are stored for multiple 米哈游账号 using current-user Windows DPAPI in `%LOCALAPPDATA%/Genshin-Go/accounts-passport.dpapi`; reopening the launcher restores the account list. Use **检查授权** to check a saved account, or **重新扫码** to renew one. Saved 米游社 authorization is intended for future check-in and wish history features. Sign in to the game in the official client after clicking **普通启动**. Authorization is not portable between Windows users or computers.

See the [米游社 authorization validation](docs/validation/t02.md) and the [T03 decision and validation record](docs/validation/t03.md).

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
Windows accepted the request; it does not claim the game reached its login screen.
Game downloads belong to a later ticket.

The isolated interaction suite runs the same Rust launcher with a controlled
application environment. It never launches the real game or writes the user's
settings:

```sh
nsh check
nsh test
cd tests/.fixture
nsh check
nsh test
```

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
