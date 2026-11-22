# Snap Hutao Island 与 Fufu UnlockerIsland 选型

**建议先验证固定版本 Fufu，在 Rust 侧自研版本管理和结果展示。两个上游的原样行为都不足以满足第六批验收。** Fufu 提供独立 Windows 启动程序，集成时可以保留现有普通启动入口。Hutao 在状态反馈和按游戏版本交付上更完整，适合参考。若 Fufu 无法在可接受的维护范围内报告失败或满足发行条件，应保留普通启动，重新评估上游选择。[产品范围](../launcher-scope.md) · [T26](../specs/launcher-tickets/T26.md)

## 两个上游如何工作

两者都让原生 DLL 在游戏进程内执行，依赖游戏内部实现提供帧率、镜头或界面等功能。游戏更新后，模块能够加载和某项效果仍然有效是两件需要分别验证的事。两家组织都提供源码。用户给出的 Hutao 链接只覆盖宿主侧，Fufu 链接则同时包含宿主入口和游戏内模块。[Hutao 原生工程](https://github.com/SnapHutaoRemasteringProject/Snap.Hutao.Remastered.UnlockerIsland/tree/1188db98fcc8cab2a349ba3962b48ac249c2fc80) · [Fufu 工程](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland/tree/1830f9d770ecb9c895ba98b9a933973eabe36e7a)

### Snap Hutao Remastered

- **版本与交付。** 指定提交的 C# 宿主在启动前读取安装版本，发现版本冲突、无法识别版本或缺少对应 DLL 时报告错误。原生模块在另一 C++ 仓库维护。内容仓库快照可见 7.2.0 和 7.3.0 两份 DLL 及 FullTrust 辅助程序。文件名证明上游按版本交付，源码与这些二进制的逐一对应关系仍待核实。[宿主版本检查](https://github.com/SnapHutaoRemasteringProject/Snap.Hutao.Remastered/blob/b0fa7ce2141abc31fb7e3ae2b65951e0ab40153a/src/Snap.Hutao.Remastered/Snap.Hutao.Remastered/Service/Game/Island/GameIslandInterop.cs) · [内容仓库快照](https://github.com/SnapHutaoRemasteringProject/Snap.ContentDelivery/tree/4baf02efece300ae5235c55d609e0e693bcb41e0)
- **运行与反馈。** 宿主借助 FullTrust 辅助进程管理扩展启动，并通过共享内存持续写入设置、读取模块状态和 UID，检查双方共享结构的大小。这使界面有依据区分进程已启动、模块已响应和协议不匹配。单项游戏效果仍要逐项验收。[宿主交互](https://github.com/SnapHutaoRemasteringProject/Snap.Hutao.Remastered/blob/b0fa7ce2141abc31fb7e3ae2b65951e0ab40153a/src/Snap.Hutao.Remastered/Snap.Hutao.Remastered/Service/Game/Island/GameIslandInterop.cs) · [共享设置](https://github.com/SnapHutaoRemasteringProject/Snap.Hutao.Remastered/blob/b0fa7ce2141abc31fb7e3ae2b65951e0ab40153a/src/Snap.Hutao.Remastered/Snap.Hutao.Remastered/Service/Game/Island/IslandEnvironment.cs)
- **代价。** 移植宿主要承担进程生命周期、二进制协议及版本对应关系。原宿主在 UID 变化时还会查询 Hutao 服务，在特定校验失败或处理器异常时可以结束游戏。这与本项目把游戏控制权留给用户的约定有直接关系。宿主和原生源码均标注 MIT，随包辅助程序及第三方依赖仍需核对来源和声明。[启动处理器](https://github.com/SnapHutaoRemasteringProject/Snap.Hutao.Remastered/blob/b0fa7ce2141abc31fb7e3ae2b65951e0ab40153a/src/Snap.Hutao.Remastered/Snap.Hutao.Remastered/Service/Game/Launching/Handler/LaunchExecutionGameIslandHandler.cs) · [UID 校验](https://github.com/SnapHutaoRemasteringProject/Snap.Hutao.Remastered/blob/b0fa7ce2141abc31fb7e3ae2b65951e0ab40153a/src/Snap.Hutao.Remastered/Snap.Hutao.Remastered/Service/Game/Island/GameIslandInterop.cs) · [宿主许可](https://github.com/SnapHutaoRemasteringProject/Snap.Hutao.Remastered/blob/b0fa7ce2141abc31fb7e3ae2b65951e0ab40153a/LICENSE) · [原生许可](https://github.com/SnapHutaoRemasteringProject/Snap.Hutao.Remastered.UnlockerIsland/blob/1188db98fcc8cab2a349ba3962b48ac249c2fc80/LICENSE.txt)

### FufuLauncher.UnlockerIsland

- **组件与功能。** 固定提交提供导出 C 接口的 `Launcher` DLL、独立的 `Launcher_2` EXE 和游戏内功能 DLL。启动组件发现插件目录中的配置，功能 DLL 读取自己的 INI 并支持重载，按游戏内部特征匹配相关功能。复用 EXE 可减少 Rust 侧的跨语言调用工作。游戏内部变化仍可能使个别功能失效。[工程配置](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland/blob/1830f9d770ecb9c895ba98b9a933973eabe36e7a/FufuLauncher.UnlockerIsland.sln) · [宿主接口](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland/blob/1830f9d770ecb9c895ba98b9a933973eabe36e7a/Launcher/Launcher.h) · [配置](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland/blob/1830f9d770ecb9c895ba98b9a933973eabe36e7a/FufuLauncher.UnlockerIsland/Config/Config.cpp) · [功能模块](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland/blob/1830f9d770ecb9c895ba98b9a933973eabe36e7a/FufuLauncher.UnlockerIsland/Core/Hooks.cpp)
- **结果语义。** 在这次核查的提交中，DLL 接口虽然接收模块路径参数，实际启动函数会扫描插件目录；独立 EXE 也扫描配置并安排加载。两个入口都可能在没有加载任何插件时继续运行游戏并返回 0。仅包装退出码会让用户看到虚假的成功，还可能在点击普通启动时重复拉起游戏。这是接入前最重要的接口缺口。[DLL 实现](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland/blob/1830f9d770ecb9c895ba98b9a933973eabe36e7a/Launcher/Launcher.cpp) · [EXE 实现](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland/blob/1830f9d770ecb9c895ba98b9a933973eabe36e7a/Launcher_2/Launcher_2.cpp)
- **依赖与发行。** 游戏内 DLL 有远端公告和访问状态检查，在特定拒绝结果下会结束宿主游戏。组件库标注 AGPL-3.0，且包含预编译 MinHook 库；公开源码与提供可分发、能追溯至特定源码的二进制是两项独立核查。检查时组件仓库没有 GitHub Release，主启动器和插件仓库另有发布渠道。若随 Genshin-Go 分发，应按实际组合方式核对许可证告知和对应源码义务，独立进程本身不能决定许可边界。[运行时入口](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland/blob/1830f9d770ecb9c895ba98b9a933973eabe36e7a/FufuLauncher.UnlockerIsland/Core/dllmain.cpp) · [许可](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland/blob/1830f9d770ecb9c895ba98b9a933973eabe36e7a/LICENSE) · [组件 Releases](https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland/releases) · [上游插件仓库](https://github.com/FufuLauncher/FufuLauncher--Plugins)

本次为静态源码和仓库产物目录核查。没有执行上游二进制或启动真实游戏，也没有验证账号风险、具体版本兼容性和发布包的源码对应关系。

## Genshin-Go 已经确定的边界

第六批的原始选择是 Fufu UnlockerIsland。T26 要求在 Windows 国服官服上验收固定版本，默认关闭，用户主动启用。失败时显示原因，由用户决定是否普通启动。T27 要求候选更新先验证兼容性，再让用户选择是否升级。项目承诺维护启动器侧适配，公开源码及提供构建产物，最终许可证仍待复用边界核实。更换上游或自行维护游戏内模块会改变现行规格。[产品范围](../launcher-scope.md) · [T26](../specs/launcher-tickets/T26.md) · [T27](../specs/launcher-tickets/T27.md)

当前 Rust 启动器校验游戏目录后，通过 Windows Shell 打开 `YuanShen.exe`。界面的“已请求启动游戏”表示系统接受请求。启用 GGScanner 时，启动器还可监测游戏退出并按 UID 同步资料。扫描器准备超时时，现有逻辑会继续启动游戏。这条超时策略只适用于资料同步；如果可选注入失败后直接走同一策略，就会违背 T26 对用户选择的约定。[启动入口](../../src/launcher.rs) · [Windows 进程实现](../../src/native_environment/process.rs) · [扫描器状态处理](../../src/scanner.rs) · [用户说明](../../README.md)

## 自研的合理范围

建议自研 Rust 启动器侧的部分，包括显示用户选用的版本及其兼容范围、保存启用设置、分别呈现游戏启动和扩展生效的结果，以及在扩展失败后提供明确的普通启动选择。这些行为可以用现有 NSH 交互测试覆盖。游戏内功能是否真正生效，仍需在指定 Windows 国服游戏版本上单独验收。[第六批验收要求](../specs/launcher-phase-one.md) · [平台边界](../adr/0001-local-data-and-platform-boundary.md)

自研游戏内模块意味着每次游戏更新后，项目同时承担内部适配、回归和发行工作。现行规格尚未把具体游戏内效果列为验收清单。若后续考虑采用自研方案，先确定玩家需要的具体效果、可接受的更新等待时间及维护人，才有依据比较长期成本。单凭本项目使用 Rust，不足以证明重写更合算。[第一期规格](../specs/launcher-phase-one.md) · [项目依赖](../../Cargo.toml)
