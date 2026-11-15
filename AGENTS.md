# 项目协作指引

## 开始修改前

- 使用中文沟通。先阅读 `README.md`，按任务检查相关源码与配置；以实际代码为准。
- 当前项目是 Rust + NSH 的应用骨架。新增业务功能时遵循用户需求，不从项目名称推断未定义的业务规则。
- 保持改动聚焦；新增依赖前先检查现有依赖能否满足需求。

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

### Issue tracker

规格与任务使用 GitHub Issues：IannTao/genshin-go。
操作约定见 `docs/agents/issue-tracker.md`。

### Triage labels

使用五个默认 triage 标签。
映射见 `docs/agents/triage-labels.md`。

### Domain docs

采用单领域布局：根目录 GLOSSARY.md 与 docs/adr/。
读取规则见 `docs/agents/domain.md`。
