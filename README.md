# Genshin-Go

On Windows, use **首页** to confirm an existing installation and launch the game. **管理游戏** opens **游戏管理** for directory changes and downloads. Open **米游社** for account authorization and community sign-in, or **角色资料** to manage local UID data spaces.

Open **祈愿分析**, **背包仓库**, or **成就管理** from the sidebar to work with data for the
selected UID. Open **系统设置 → 备份与恢复** to move data across installations.

## Windows curated plugin catalog

Open **扩展** to browse and manage the curated plugin catalog maintained by Genshin-Go.
The launcher does not connect to arbitrary third-party store sources. The initial confirmed
entry is [FufuLauncher.UnlockerIsland](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland)
(pinned commit `1830f9d7`, 2026-11-18). The panel displays its verified type, version,
upstream source, managed install location under `plugins/fufu-unlocker-island`, required
administrator privileges and risk notice, compatible local module files
(`FufuLauncher.UnlockerIsland.dll` and `config.ini`), and AGPL-3.0 license notice.
Use **安装插件**, **检查并更新** (manual only; no automatic updates), and **卸载插件**
against the configured local source directory (default
`C:\softwares\FufuLauncher\Plugins\FuFuPlugin`). Installed state and content SHA-256
digest are saved in the managed plugin directory and verified on reopen. If catalog
loading fails, the page reports the error and offers **重新加载目录** to retry.

## Windows owned character sync

In **游戏管理**, select the Windows capture edition of `GGScanner.exe` (compatibility
baseline v2026.11.15) and its local port (default 8765). In GGScanner, select
Genshin and enable **持续同步（抓包）**. The scanner requires administrator
privileges and its third-party capture carries account risk. Start the manager
service to test the port with **检查扫描连接**, then choose **保存扫描设置** to
enable sync.

Click **普通启动** and log into the game. The launcher reuses a running Genshin
manager or starts the selected GGScanner in `--agent-mode`. On a cold start,
approve the Windows PowerShell administrator prompt if it appears. The helper
starts GGScanner and can request normal closure of that instance at the end of
the sync. Cancel the Windows prompt to continue without scanner authorization.
While waiting in the launcher, choose **直接启动游戏** to skip sync. After 15
seconds the launcher proceeds with game launch and reports the scanner timeout
separately.
Use **取消本次同步** after launch to stop waiting for the login. Cancellation or
game exit requests normal closure of the dedicated scanner. A scanner failure
appears alongside the game launch result. Open **角色资料** and select the UID
used in the game to view its owned characters, elemental forms, progression,
source time and local receipt time. **停用下次启动同步** disables future scans.

## Windows achievement progress

Create or switch UID data spaces in **角色资料**, then open **成就管理** to browse the offline
Simplified Chinese achievement catalog. Search by name or ID and use **标记完成** or
**取消完成** to update the selected UID's completion status. Multi-stage achievements
show the stage and its target value. Changes are saved in
local SQLite and work without signing in. Manual edits keep any recorded numeric progress.
The catalog contains 1,909 IDs from the [genshin-db v5.2.16 release for game version 7.2](https://github.com/theBowja/genshin-db/releases/tag/v5.2.16).

To exchange [UIAF v1.1 files](https://uigf.org/zh/standards/uiaf.html), select a JSON file
or enter its path, choose the destination UID, and review additions, updates, duplicates,
and conflicts before clicking **备份并确认导入**. Imported records for IDs outside the
catalog appear with their IDs.

The merge keeps the strongest completion status, the greatest progress, and the earliest
known completion time from completed records. Reimporting identical data leaves progress
unchanged. Backups and exports appear under `achievement-backups` and `achievement-exports`
in the launcher data directory. They include `info.uid` as optional metadata so the
launcher can identify the destination on reimport. A file without that field always
requires an explicit UID selection. Missing completion times use the UIAF unknown-time
value.

## Windows backup and restore

Open **系统设置 → 备份与恢复** and choose **创建整体备份**. Copy the displayed JSON file
from `uid-backups` in the launcher data directory to the new computer. The file
contains all UID data spaces, wish records, achievement progress, character and
weapon growth plans, manual material quantities, and inventory snapshot
history. Scan again in **米游社** on the new computer to authorize community features.

Select the JSON file or enter its path, then choose **预览恢复**. The preview
shows new UID data spaces and counts of new, duplicate and conflicting records
for each UID. **备份并确认恢复** saves the destination's current data to a new
file in `uid-backups` before merging. Choose the local or backup version for each
conflicting plan or manual material quantity before confirming. Wishes are deduplicated
by record ID and conflicting wishes keep the local copy. Achievement progress merges
by status, progress and completion time using the same rules as UIAF import.
Inventory quantities stay with their original snapshots and collection times.
**背包仓库** opens the latest snapshot available for each category. Select an older snapshot
in **背包快照历史** to view it. If two snapshots with the same collection time differ,
the restore preview reports the conflict and keeps the local snapshot.

The launcher can restore version 1 and version 2 backups. Restoring a version 1
file preserves existing plans and inventory history. Restore keeps the selected
UID if there is one. Use the UIGF and UIAF controls for standard exchange files.

## Windows inventory snapshots

Open **背包仓库** to browse weapons, artifacts and materials for the
selected game UID. Select a category to see its latest available snapshot. Use **背包快照历史**
to view an older snapshot. Each view shows the original collection time and
the inventory from that snapshot.

Select or enter the path to a GOOD JSON export covering any of weapons,
artifacts and materials. Choose **预览背包导入至 UID** for the appropriate data
space and verify each supplied category. Choose **备份并确认导入所含类别** for a
partial export or **备份并确认完整导入** for a complete one. Omitted categories keep
their latest available snapshots. An explicit empty list or material map saves
that category as empty.

Inventory Kamera exports GOOD v2 and can collect equipment and materials from the
game window. Its original filename supplies the local export time. Confirm the
collection time and add its UTC offset, such as `+08:00`. When a category is
omitted, you can import the supplied categories immediately or include the
omitted category after verifying that its game inventory is empty.
[Inventory Kamera](https://github.com/Andrewthe13th/Inventory_Kamera)

The launcher reads the UID and collection time declared by a GOOD v3 Leyline
export. For GGScanner exports named `genshin_export_yyyy-MM-dd_HH-mm-ss.json`,
the launcher suggests the local export time so you can confirm it and add its
UTC offset. For other GOOD files, `metadata.uid` and `metadata.capturedAt` can
supply the same information. [Leyline format](https://teyvat.me/developers#the-file)

The inventory uses each snapshot's recorded quantities. Before importing, the
launcher backs up the UID's current snapshots for the supplied categories to
`inventory-backups` in its data directory. A previous snapshot backup can be
selected through the GOOD importer.
The first import records that the UID had no inventory yet.

When the selected UID matches the authorized 米哈游账号, choose **从米游社同步材料**
in **背包仓库** to fetch character and weapon cultivation materials from the official
米游社 calculator (`e20200928calculate/v3/batch_compute`). The launcher backs up the
previous material snapshot, saves the synchronized quantities as a materials-only
snapshot (`MiYouShe-Calculator`), preserves existing weapon and artifact snapshots,
and makes the updated materials immediately available to **使用导入库存** in the growth
planners.

## Windows character growth plans

Open **角色资料 → 角色养成计划**, choose a UID, and search for a character. Enter
the current and target level, completed ascension phase (0–6), and base levels
for normal attack, elemental skill and elemental burst. A character at 20/20
has phase 0, and a character at 20/40 has phase 1. The panel shows the required
materials and updates each shortfall as you enter manual stock. Click
**保存养成计划** to save the plan and the displayed stock edits for the selected UID.
Saved plans reopen without signing in. Manual stock is shared among that UID's
plans. **删除历史记录** clears both the plans and manual stock for the selected UID.

The planner calculates character EXP from the start of the entered level. It
rounds each segment ending at an ascension cap or the target level up to a whole
1,000 EXP book unit, and estimates 1 Mora per 5 EXP in that total. In-game
costs can vary with existing EXP and the books you use. See the
[official notes on EXP overflow](https://genshin.hoyoverse.com/en/news/detail/125553).
Traveler forms have separate plans for their separate talents. Each form's calculation uses
the level and ascension you enter for that plan, which you can update after
advancing Traveler in the game.

The offline Simplified Chinese catalog uses the character and talent costs in
[genshin-db v5.2.16 for Genshin Impact 7.2](https://github.com/theBowja/genshin-db/releases/tag/v5.2.16)
under the [bundled MIT notice](assets/achievement-catalog-LICENSE.txt).
The experience curve comes from the [per-level progression table](https://github.com/devesh-anand/genshin-api/blob/586626c333cffa8c4eeb3e78a40bf642d0abbdb9/data/progression.json)
and uses the [official 95/100 rules](https://genshin.hoyoverse.com/en/news/detail/159349)
for levels above 90.

## Windows weapon growth plans

Open **角色资料 → 武器养成计划**, select a UID, and search by weapon name or ID.
Enter the current and target level and completed ascension phase. For example,
20/20 is phase 0 and 20/40 is phase 1. One- and two-star weapons cap at level 70
and phase 4. Standard three- to five-star weapons cap at level 90 and phase 6.
For the Exaiphanes Blade, use the displayed quest levels. Its progression is
unlocked through the story and Statues of The Seven in Snezhnaya.
[Version 7.0 update details](https://weibo.com/ttarticle/p/show?id=2309405330984475427108)

Enter manual stock to see the remaining requirements. **保存养成计划** saves the
plan and its displayed stock edits. Character and weapon plans share the manual
stock of the selected UID. **删除历史记录** clears those plans and quantities for
that UID.

The planner starts at the beginning of the entered level. It rounds each segment
ending at an ascension cap or the target level up to 400 EXP and estimates 1 Mora
per 10 EXP in the rounded total. For example, raising a White Tassel from level 1
to level 21 needs 60,400 EXP, rounded to 60,800 EXP. With the first ascension,
the plan requires 11,080 Mora and 2 Luminous Sands from Guyun.

The Simplified Chinese names and ascension costs come from
[genshin-db v5.2.16 for Genshin Impact 7.2](https://github.com/theBowja/genshin-db/releases/tag/v5.2.16)
under the [bundled MIT notice](assets/achievement-catalog-LICENSE.txt). Weapon EXP
curves use the [per-level game data](https://gitlab.com/GuraFoundation/YuanShenResources/-/blob/6044f5aae7dd79fbbe15c5748ef99b654f6132b7/ExcelBinOutput/WeaponLevelExcelConfigData.json).
The Exaiphanes Blade's level milestones use its
[enhancement table](https://gitlab.com/GuraFoundation/YuanShenResources/-/blob/6044f5aae7dd79fbbe15c5748ef99b654f6132b7/ExcelBinOutput/WeaponQuestSnezhnayaEnhanceExcelConfigData.json).

## Windows inventory stock for growth plans

In either growth planner, choose **使用导入库存** to calculate shortfalls from
the selected UID's latest GOOD snapshot containing materials. The editor shows the original
collection time and imported quantities. A missing material has zero stock.
Choose **使用手填库存** to return to your manual quantities. Each editor opens
in manual mode, and **保存养成计划** saves manual stock edits while using manual
mode. Imported quantities are read-only.

The offline material lookup matches game IDs to GOOD keys from the
[Genshin Optimizer material ID map](https://github.com/frzyc/genshin-optimizer/blob/f24bcb82f650ff755af9ed47956a91809f142772/libs/gi/dm/src/dm/material/MaterialExcelConfigData_idmap_gen.json).

## Windows account authorization

The launcher uses the [UIGF Rust SDK](https://github.com/UIGF-org/mihoyo-api-sdk) for Passport App QR login and session verification. After confirmation it exchanges the SToken for account cookies and checks that the account has a unique official mainland-China Genshin UID before storing the authorization.

Use **扫码添加米哈游账号** to create a new QR code and confirm it in 米游社. The UI shows scanning, confirmation, expiry, cancellation and network errors. Expired codes can be refreshed. The launcher stores one authorization in `%LOCALAPPDATA%/Genshin-Go/launcher.sqlite3`, with the Cookie encrypted using current-user Windows DPAPI. Reopening restores the account and its verified UID. In **管理授权**, choose **检查授权** to check the saved account. Choose **重新扫码** on the 米游社 page to renew it with the same account and UID. Sign in to the game in the official client after clicking **普通启动**. Authorization is not portable between Windows users or computers.

Scan authorization sets up a data space for the game UID verified by 米游社 and keeps any existing space selection. To add another character in **角色资料**, enter its game UID and choose **创建 UID 数据空间**. You can create and switch spaces without signing in. The launcher restores the selected space on reopening. See the [T04 validation record](docs/validation/t04.md).

The launcher assigns one device ID to the local database and reuses it for refreshed QR codes, new scans and authorization requests after reopening.

Upgrading from the multi-account version removes its saved authorizations. You can switch between existing UID data spaces before scanning again. To verify the upgrade with isolated Windows data, run `./docs/validation/issue-32-windows.ps1` in PowerShell 7.2 or later. Add `-LiveScan` to verify a real scan and two restarts. The script removes its temporary directory after a successful run. Use `-KeepData` to retain it for inspection. A failed run leaves the directory in place for diagnosis.

In **管理授权**, choose **移除账号** and confirm the displayed account to remove its 米游社 authorization. Automatic sign-in requires authorization again. The UID spaces and their records stay available. Scanning again verifies the account's game UID and links it to the existing space.

To clear local records, select the UID in **角色资料**, open **资料管理**, and choose **删除历史记录**. Confirm the full UID displayed to clear every category of local history for that UID. The UID space, other UIDs' records, backup and export files, and 米游社 authorization stay available. Keyboard users can Tab through controls in reading order; in both Windows confirmation dialogs, Tab stays inside the dialog, Enter defaults to **取消**, Esc returns safely, and focus returns to the trigger button.

## Windows community sign-in

The sole saved 米哈游账号 is the sign-in account. The launcher shows **社区签到** and checks for the day's unfinished community sign-in when opened, at Beijing midnight, and after Windows resumes. Switch UID data spaces independently. The launcher only runs checks while it is open.

Open **米游社** to see the current 米哈游账号, today's sign-in result, and the **社区签到奖励日历** at the top of the page. The calendar shows the past seven Beijing dates, including results checked during the current session and `未检查` for unchecked dates. It displays the reward amount from the 米游社 task definition when 米游社 confirms delivery. If sign-in succeeds and delivery is still unconfirmed, use **检查奖励领取** next to today's result to refresh the reward status.

Use **重试社区签到** next to today's result after a failure. If 米游社 requires verification, complete verification or sign-in in the app, then retry. If the launcher asks you to scan again, use **重新扫码** next to the result to renew the same account. Older authorizations may need a fresh scan to supply the separately encrypted community sign-in session. See the [T11 sign-in validation](docs/validation/t11.md) and [T13 calendar validation](docs/validation/t13.md).

## Windows launcher: existing official mainland-China installation

The Rust root discovers existing installations from the current-user and machine
uninstall registry entries (both registry views), legacy launcher `config.ini`
paths, and common Program Files locations. The home page shows the complete path
of a single candidate or lets you choose among multiple candidates. Click
**确认保存目录** to set the default and reveal **普通启动**. If automatic discovery
finds no candidates, use **选择已有游戏** to browse or **下载安装** to choose a new
installation location.

**游戏管理** shows the current default directory. Use **更换目录** to find moved
installations with **重新查找**, browse with **选择游戏目录**, or enter a path and
click **使用此目录**. A valid game directory contains `YuanShen.exe`,
`YuanShen_Data`, and an official-channel `config.ini`. Choosing a legacy launcher
directory also resolves its game path. A failed save keeps the previous default.

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
Windows CI runs the same script. The root tests use a fresh `GENSHIN_GO_DATA_DIR`
that the script removes after the run. NSH keeps its normal cache location.
When running `nsh test` or `nsh test nsh.json` directly against the Rust root, set
`GENSHIN_GO_DATA_DIR` to an absolute path to an isolated directory first.
From `tests/.fixture`, run `nsh test .windows/authorization.test.ts --target windows`
to verify that Enter cancels account removal in the Windows window.
Run `nsh test .windows/history.test.ts --target windows` to verify that Enter
cancels history deletion.
Run the [live account validation](docs/validation/t02.md#重跑验证) separately.

Windows storage tests create their own temporary databases and use DPAPI under
the current Windows user. If a sandboxed test reports `Windows 无法保护或解密凭据`,
rerun the same command from that user's PowerShell session with access to DPAPI.
To run the isolated storage tests alone, use
`cargo test --locked --lib account_store::tests::`.
For a read-only schema and UID space check, see the
[SQLite diagnostic commands](docs/validation/t04.md#sqlite-结构检查).

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
nsh test                     # headless root tests; configure GENSHIN_GO_DATA_DIR first
nsh test nsh.json --trace    # root mount trace; configure GENSHIN_GO_DATA_DIR first
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

Open the interactive Windows launcher prototype:

```powershell
Start-Process .\src\launcher-prototype.html
```

Use **演示场景** in the lower left to try first-time setup, an interrupted download, or an expired authorization. Downloads, QR confirmations, and UID records use in-memory sample data. See the [interaction design](docs/specs/launcher-ui-design.md).

Open the earlier three-layout comparison:

```powershell
Start-Process .\src\ui-prototype.html
```

Use the floating arrows or the left and right arrow keys to compare variants A, B and C. The URL's `?variant=` value keeps the selected layout on reload. Account, directory, sign-in and launch controls are local demonstrations; the production Rust UI remains the app entry.
