# 项目协作指引

## 入口与边界

- 使用中文沟通。开始前按任务阅读 `README.md` 及相关源码，以实际代码为准。
- `src/lib.rs` 定义 Rust UI 根组件 `App`，并通过 `#[native_module("genshin-go/native")]` 注册供 TSX 使用的组件；`nsh.json` 的 `root` 与其对应。
- `src/main.rs` 是桌面原生启动入口，修改平台逻辑时保留各平台条件编译边界。默认入口保持 Rust UI；新增 TSX 入口见 `README.md` 的 “Adding a TSX entry”。
- `.nsh/` 与 `target/` 是生成目录，修改源码与根目录配置而非生成产物；升级 NSH 时保持 `nsh-*` 依赖与 `nsh` CLI 版本一致。
- Windows 缺少 `rg` 时，用 `git grep -n` 定位已跟踪源码，用 `git log -G` 查找旧格式或旧路径。

## 验证与交付

- 全量验收入口：`./scripts/check-windows.ps1`（依次执行 `cargo fmt --check`、`cargo clippy --locked --workspace --all-targets -- -D warnings`、`cargo test --locked --lib`、Issue 同步离线测试、根项目与 `tests/.fixture` 的 `nsh check` 与 `nsh test`）。
- 增量开发可单独运行 `cargo fmt`、`cargo test --locked --lib <filter>`、`nsh check` 或 `nsh test`；运行根目录 `nsh test` 时使用独立临时 `GENSHIN_GO_DATA_DIR`。
- 交付时说明改动结果、实际执行的验证及未完成的检查；环境或工具受限时报告具体原因。

## 规范与工作流指针

- **新功能 / 产品行为 / UI 变更**：先读取并遵循 [.agents/skills/genshin-go-workflow/SKILL.md](.agents/skills/genshin-go-workflow/SKILL.md)。
- **Issue 跟踪与回写**：仓库 `IannTao/genshin-go`，读取与回写规则见 [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md)；标签映射见 [docs/agents/triage-labels.md](docs/agents/triage-labels.md)。
- **领域术语与架构决策**：单领域布局（`GLOSSARY.md` 与 `docs/adr/`），读取规则见 [docs/agents/domain.md](docs/agents/domain.md)。
- **代码审查标准**：判断性规范见 [CODING_STANDARDS.md](CODING_STANDARDS.md)（机械 lint 门禁由 `Cargo.toml` 的 `[workspace.lints]` 强制执行）。
