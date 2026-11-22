# Windows 启动原神时同步 GGScanner 数据

**建议接入 GGScanner 的本机持续同步接口。** 点击「普通启动」时先准备扫描器，再请求启动游戏。玩家登录后，启动器按实际游戏 UID 将成就、已拥有角色、武器、圣遗物分别写入 SQLite，每收到一个完整类别就保存该类。技术上可以覆盖成就、已拥有角色、武器和圣遗物；能否做到每次都只需点一次启动，取决于扫描器冷启动和成就数据到达时机，需 Windows 实测。

## 上游能力与版本

2026-11-21 核对的最新正式版为 `v2026.11.15`（提交 `4274dff`，构建 99）。该版新增持续同步，并为单次和持续快照校验登录 UID。Windows 应使用 `GGScanner.exe`。同版 `GGScannerOCR.exe` 提供前台 OCR；其持续同步端点始终返回未启用。建议把这个标签和对应发行文件的 SHA-256 一起记录为首个兼容版本。[发布说明](https://github.com/Anyrainel/GGScanner/releases/tag/v2026.11.15) · [发布资产元数据](https://api.github.com/repos/Anyrainel/GGScanner/releases/tags/v2026.11.15) · [持续同步契约](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/CONTINUOUS_CAPTURE.md)

### 读取哪个接口

使用 GGScanner 在本机提供的 `GET /continuous`，默认地址为 `http://127.0.0.1:8765`，端口可配置。上游支持 Rust 等原生客户端直接读取。可选的 `GET /health` 用来诊断服务是否可达。要取得本次登录的数据，应在玩家登录前让服务就绪。先在 GGScanner 选择原神，在管理页勾选“持续同步（抓包）”，再启动管理服务。默认配置关闭持续同步。[原神管理 API](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/MANAGER_API.md) · [管理界面](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/application/src/gui/manager_tab.rs) · [配置默认值](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/genshin/src/cli.rs)

| 服务响应 | 启动器应如何处理 |
| --- | --- |
| 连接失败 | 服务未就绪时短时重试。超时后提示检查进程和配置的端口。 |
| 409 `continuous_capture_disabled` | 提示检查版本和管理页的持续同步开关。勾选后需要重新启动该服务。 |
| 503 `continuous_capture_no_data` | 服务正在等待登录或初始库存，按几秒的间隔继续轮询。 |
| 503 `continuous_capture_failed` | 本次同步失败，显示可理解的错误，保留上次成功数据。 |
| 200 | 解析 `sessionId`、`revision`、`updatedAt` 和 `data`。修订号仅在本次登录会话内有序，`updatedAt` 是内容变化的 Unix 毫秒时间。 |

同一 UID 重连也会产生新的 `sessionId`。首个 200 只说明已获得 UID 与初始库存，成就可能稍晚出现。上游每约十分钟才尝试写有变化的 `genshin_export_latest.json`，退出时不会强制写入，短会话可能没有新文件。实时消费应以 HTTP 为准。[响应、重连与落盘语义](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/CONTINUOUS_CAPTURE.md)

上游会合并可能分批到达的成就 ID。固定版持续同步响应没有单独的「全部成就已到齐」标志。实机验收应对照已知的完整成就列表检查首份和后续修订；只有能够确认完整性的结果，才能用于撤销列表外的手动完成标记。该版本如何可靠判定完整性仍待验证。[成就汇总与导出实现](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/genshin/src/capture/player_data.rs) · [持续同步响应](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/CONTINUOUS_CAPTURE.md)

### 能否自动启动

无参数启动会打开 GUI。固定版另有 `GGScanner.exe --agent-mode`，会打开 GUI 并启动上次所选游戏的管理服务，且检查管理员权限。它沿用该游戏保存的设置，所以首次配置要选中原神并开启持续同步。源码把这个参数定位为本地开发迭代入口；升级 GGScanner 前应复验它。建议先接入已经就绪的服务，再用这个固定版本验证冷启动。无需采用上游面向开发机的计划任务脚本。[参数分派](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/application/src/bin/yas.rs) · [GUI 自动开服与权限判断](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/application/src/gui/mod.rs)

OCR 也可显式选择 `--achievements`。`--all` 只涵盖角色、武器、圣遗物。OCR 需要简体中文游戏界面，会占用鼠标，默认武器最低三星、圣遗物最低四星，导出缺少可验证的登录 UID。它适合作为用户主动发起、确认归属的补录渠道。单次启动的自动同步以带 UID 的持续快照为输入。[原神命令行选项](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/genshin/src/cli.rs) · [OCR 使用条件](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/README.md) · [UID 导出契约](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/GENSHIN_EXPORT.md)

### 四类 GOOD 数据的判读

响应的 `data` 是一份原神 `format = "GOOD"`、`version = 3` 的导出。成功的该版持续快照应含字符串 `metadata.uid`，须以它确定本地 UID 空间。昵称、启动器选中的 UID、米游社账号都不能证明本次实际登录身份。以下是固定版本的字段及建议。[UID 契约](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/GENSHIN_EXPORT.md) · [GOOD 数据结构](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/genshin/src/scanner/common/models.rs) · [连续快照完整性与实例标识](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/CONTINUOUS_CAPTURE.md)

| 数据 | 导出与导入规则 |
| --- | --- |
| 成就 `achievements` | 收到的已完成 ID 可以立即保存。字段缺失或无效时保留当前记录。只有能确认整份成就列表完整时，才撤销缺席 ID 的完成标记；此时 `[]` 表示已完成集合为空。 |
| 已拥有角色 `characters` | 可选数组，保存 GOOD 角色 `key`、元素（有值时）、等级、突破、命座和天赋。保留多元素形态，养成计划的目标值独立保存。 |
| 武器 `weapons` | 可选数组，含武器 `key`、等级、突破、精炼、装备者和锁定状态。同名武器可有多把，完整快照应保留全部实例。 |
| 圣遗物 `artifacts` | 可选数组，含套装、部位、等级、主副词条、装备者等。两件相同面板的圣遗物仍可同时存在。 |

角色和装备都应区分缺失字段与已就绪的空数组。同一 UID 下只有在 `generator.itemIdentity` 等于 `account-instance-v1` 时才把装备 `_uid` 当作跨快照的稳定实例标识。首版可整类事务替换，保留重复装备和附加字段。上游注明某些单独的锁定、装备状态变化可能暂未反映，下一次登录会重新建立基线。这限制了游戏过程中逐次变更的覆盖率。[持续同步说明](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/CONTINUOUS_CAPTURE.md)

## 启动器的接入设计

**一次「普通启动」对应一次同步。** 玩家首次在 Windows 启动器选定正式版 `GGScanner.exe`、配置的本机端口并启用该功能。GGScanner 中应选中「原神」并启用「持续同步（抓包）」。首版使用玩家已有的发行文件，启动器提供版本提示和连接检查。上游发行说明提示第三方工具可能带来账号风险，首次开启时应让玩家看到采集方式和所需管理员权限。启动器沿用普通用户权限。[上游发行说明](https://github.com/Anyrainel/GGScanner/releases/tag/v2026.11.15) · [本机数据边界](../adr/0001-local-data-and-platform-boundary.md)

1. **点击启动。** 在现有 `launcher::launch` 验证安装目录后，启动后台协调任务。优先复用配置端口上的原神服务；冷启动才打开选定的扫描器。等待时间有上限，玩家可以直接继续启动游戏。扫描器缺失、端口被占用、权限请求被取消或版本不兼容时，照常请求启动游戏，首页同时给出扫描失败提示和具体原因，例如「扫描失败：未获得 GGScanner 管理员权限」。重复点击复用同一任务。当前 `LauncherEnvironment::launch` 经 `ShellExecuteW` 返回，成功只表示 Windows 接受了启动请求。[启动按钮](../../src/launcher.rs) · [Windows 启动实现](../../src/native_environment.rs)
2. **建立本次登录的边界。** 登录前先读一次 `/continuous`。若读到现有快照，记下旧 `sessionId`，必须等到新登录产生不同的 `sessionId` 才能归入本次启动；若收到 `continuous_capture_no_data`，即可请求启动原神，再等本次登录的首份数据；已有快照的分支也照常启动游戏。`/health` 可达只说明服务在运行。连接异常、服务禁用、另一个游戏的响应与等待登录分别展示。游戏启动失败则取消本次任务。已经在线且没有发生新登录时，保留原记录，不把旧快照标作刚更新。[持续同步协议](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/CONTINUOUS_CAPTURE.md) · [身份与登录代次](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/GENSHIN_EXPORT.md)
3. **接收与提交。** Rust 后台任务每隔数秒只读配置的回环地址。校验会话、修订、GOOD v3 格式和实际游戏 UID 后，逐类校验并立即保存本次已完整收到的类别。首次遇到的合法 UID 自动创建数据空间，保留玩家当前浏览的 UID。若武器最先到达，先显示「武器已更新」，其余类别各自保留上次数据与时间。GGScanner 通过抓包取得同步数据，玩家照常登录和游玩。其他三类已保存、仅成就尚未到达时，启动器内仅显示一次「已更新 3/4 类，等待成就数据」，保持游戏焦点。此时退出，显示「本次已更新 3/4 类」与成就上次更新时间；首次使用则显示「从未更新」。成就分批到达时先保存已确认的 ID，界面继续显示成就同步中。四类都确认完整并保存后结束本次任务；中途退出或超时仍保留已经提交的类别。缺少可信 UID 的快照不能自动建档或写入。[按类别保存的决策](../adr/0003-ggscanner-category-sync.md) · [UID 校验实现](../../src/profile.rs) · [持续同步的数据到达条件](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/CONTINUOUS_CAPTURE.md)
4. **恢复与去重。** 每一类的快照和该类的 UID、`sessionId`、`revision`、上游内容变更时间 `updatedAt`、本机接收时间在同一事务提交。按类别记录已处理的修订，以便同一响应中其他类别仍可独立保存；跨会话的修订号从头计数。新会话即使内容相同，也可更新「本次已核对」时间。历史删除或任务取消后，失效的任务不得重新写回。本次同步完成、玩家取消或游戏退出时，启动器仅向自己为这次同步启动的 GGScanner 实例请求正常关闭。玩家事先打开的实例继续运行。本次启动按单一登录 UID 设计，中途换号不列为产品场景。[SQLite 决策](../adr/0002-local-sqlite.md) · [UID 历史删除](../../src/account_store.rs)

### 与现有成就和背包记录衔接

玩家随时可以手动标记完成或取消完成，修改立即生效。下一次属于该 UID 的完整成就扫描覆盖当时的完成状态；即使新一次扫描的 ID 集合与上次相同，也要纠正两次扫描之间的手动修改。同一次扫描结果的重复轮询不覆盖该次扫描之后的手动修改。成就字段缺失或无效时保留当前状态。[现有手动标记](../../src/achievements/store.rs)

首版先同步完成状态。GGScanner v2026.11.15 导出的成就数组仅含 ID。已有 UIAF 完成日期照常显示，注明「来源：UIAF」，缺少日期时显示「完成时间未知」。`updatedAt` 只用于快照同步信息。后续版本若提供逐项实际完成时间，以该时间为准。[成就导出示例](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/README.md#成就导出语义) · [GOOD 结构](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/genshin/src/scanner/common/models.rs) · [时间字段定义](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/docs/CONTINUOUS_CAPTURE.md)

角色和装备可先保存结构校验后的 GOOD 数据，再从最新快照生成只读视图。数据表外键接入 `uid_history(uid)` 的级联删除。启动器目前已有分 UID 成就合并和备份，整体备份 v1 仅收录祈愿和成就。增加快照时升级备份格式并继续读取 v1，旧备份缺失快照时保留已有数据。手动文件导入继续预览、备份。自动同步取得完整武器或圣遗物列表后，立即按 UID 替换该类别，并保留该类上一份可恢复记录。列表为空时库存清零，缺少该字段时沿用原库存和时间。T19 现有规格要求文件导入前预览和备份；实施前应将这条自动同步规则明确写入规格。T19 还要求材料，而本次请求的四类及此 GOOD 导出都不含材料。[GOOD 数据模型](https://github.com/Anyrainel/GGScanner/blob/v2026.11.15/genshin/src/scanner/common/models.rs) · [整体备份](../../src/backup.rs) · [背包验收范围](../specs/launcher-tickets/T19.md)

### 实施顺序与通过条件

| 阶段 | 可验收的结果 |
| --- | --- |
| Windows 可行性验证 | 使用固定发行版和国服官服客户端，分别从扫描器已开和完全关闭开始，由同一个「普通启动」入口启动。记录 UAC、登录前就绪情况、成就是否无需操作就到达，以及四类数量与游戏内抽查结果。切换至另一游戏 UID 并再次启动，确认不会沿用上一个账号。玩家仅正常登录并游玩，全程不进入成就页。若本次未收到成就，记录为仅部分同步；若冷启动需要玩家额外操作，记录具体步骤。 |
| 本机导入 | 使用脱敏的真实响应及隔离 SQLite，验证缺字段与空数组、同名装备多件、装备消耗、已有 UIAF 进度、同 UID 重连、跨 UID、过期修订、重复提交、历史删除以及 v1/v2 备份恢复。单类业务数据和该类修订应同时提交或同时回滚。先仅返回武器，再补齐其余类别，验证第一次响应后武器已保存。再验证两次相同的完整成就扫描之间，手动修改先即时生效、随后被第二次扫描覆盖；缺少成就字段时保留手动修改，分批到达时立即标记已收到的 ID，列表完整性未确认时保留其他标记。验证含 UIAF 日期和无日期两种显示结果，快照同步时间不能写入成就完成时间。 |
| 启动器交互 | 将本机探测与读快照作为可替换的 Windows 环境能力，使用已有 `reqwest`、`serde_json`、`rusqlite` 即可。通过 NSH fixture 验证正常启动、等待成就、扫描器缺失、提权取消、重复点击和失败后下次重试。首页显示游戏启动结果，角色资料显示实际 UID、四类条数、上次成功时间及本次结果。 |

依据为固定标签的文档和源码，以及本仓库当前工作树。此次只完成接口与代码核对；Windows 真机冷启动、扫描器正常关闭、当前客户端数据完整性、首次使用所需操作及不同启动之间的 UID 归属列入第一阶段实测。[现有交互测试入口](../../tests/.fixture/App.test.tsx) · [项目验证约定](../../README.md)
