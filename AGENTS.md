# 项目协作指引

## 开始修改前

- 使用中文沟通。先阅读 `README.md`，按任务检查相关源码与配置；以实际代码为准。
- 当前项目是 Rust + NSH 的应用骨架。新增业务功能时遵循用户需求，不从项目名称推断未定义的业务规则。
- 保持改动聚焦；新增依赖前先检查现有依赖能否满足需求。
- Windows 上缺少 `rg` 时，用 `git grep -n` 定位已跟踪源码，用 `git log -G` 查找旧格式或旧路径，再读取命中的片段。

## 入口与边界

- `src/lib.rs` 定义 Rust UI 根组件 `App`，并通过 `#[native_module("genshin-go/native")]` 注册供 TSX 使用的组件。
- `nsh.json` 的 `root` 与 Rust 根组件对应。修改根组件名称或导出时，同步检查配置、注册和调用方。
- `src/main.rs` 是桌面原生启动入口。修改平台启动逻辑时保留各平台的条件编译边界；其他平台的运行和打包方式见 `README.md`。
- 默认入口保持 Rust UI。需要新增 TSX 入口时，阅读 `README.md` 的 “Adding a TSX entry”，按其中的注册和类型生成流程操作。
- 修改 NSH 版本时，保持 `nsh-*` 依赖与使用的 `nsh` CLI 版本一致。

## 源文件与生成文件

- 修改源码和根目录配置；`.nsh/` 与 `target/` 是生成目录，不直接修改其中内容来实现功能。
- `tsconfig.json` 依赖 `.nsh/tsconfig.json`。缺少生成配置或类型时，通过 NSH 的构建流程生成，具体入口见 `README.md`。
- `Cargo.lock` 随依赖变动更新，避免为无关修改刷新依赖。

## 验证与交付

- 修改 Rust 后运行 `cargo fmt --check`；需要格式化时运行 `cargo fmt`。
- 修改代码、配置或依赖后运行 `nsh check`；修改交互行为时运行 `nsh test`，并按需更新 `App.test.tsx` 中的行为断言。Rust 逻辑测试可用 `cargo test` 单独运行。
- 验证平台专属改动时使用相应目标；无对应运行环境时明确说明验证范围。
- 仅修改文档时，检查命令、路径和描述与仓库一致，无需构建应用。
- 交付时说明改动结果、实际执行的验证及未完成的检查；工具或环境缺失时报告具体原因。

## Agent skills

### 需求到任务工作流

当用户提出 Genshin-Go 的新功能、产品行为或 UI/UE 变更需求，或点名 `genshin-go-workflow` 时，先读取并遵循 [genshin-go-workflow](.agents/skills/genshin-go-workflow/SKILL.md)。收到需求后，逐阶段自查并获得用户确认，再开始下一阶段。仅要求执行已有 Issue 时，遵循下方任务跟踪约定。

### Issue tracker

规格与任务使用 GitHub Issues：IannTao/genshin-go。
每完成一个 Issue，都要按[任务跟踪约定](docs/agents/issue-tracker.md)回写对应的 GitHub Issue，再报告交付。

### Triage labels

使用五个默认 triage 标签。
映射见 `docs/agents/triage-labels.md`。

### Domain docs

采用单领域布局：根目录 GLOSSARY.md 与 docs/adr/。
读取规则见 `docs/agents/domain.md`。

## Rust编码规范

### **可见性**：默认使用私有（`fn`/`struct`），仅在必要时提升为 `pub(crate)` 或 `pub`。禁止滥用 `pub use` 破坏模块边界。

### **注释**： 所有 pub 的 struct、enum、trait、函数、方法、常量都必须有 /// 注释。AI 生成的新公开项必须同步补文档注释。文档注释用第三人称陈述句：/// Returns the underlying value. 而非 /// Get the underlying value。首句以句号结尾，首句应独立说明该项是什么或做什么。

### **lint**：禁止新增 #[allow(clippy::...)]。确需抑制时只能用 #[expect(<lint>, reason = "具体原因")]。CI 以 cargo clippy --workspace --all-targets -- -D warnings 为门禁，任何新增警告都会阻断合并。
