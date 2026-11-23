# Coding Standards

仅保留编译器与 Clippy 无法替代的判断性标准，供 `code-review` 阶段核对。机械门禁见 `Cargo.toml`（`[workspace.lints]`）与 `scripts/check-windows.ps1`。

## Rust 模块与接口

- **最小可见性**：默认私有，仅在跨模块必需时提升为 `pub(super)`、`pub(crate)` 或 `pub`；避免用 `pub use` 重新导出内部实现细节。
- **公开文档注释风格**：`pub` 项的 `///` 首句使用第三人称陈述句并以句号结尾（如 `/// Returns the underlying value.`），独立说明该项是什么或做什么。
- **状态与参数内聚**：避免多处重复 `match` 同一枚举（提取元数据表/结构），避免在 UI 装配函数间透传大组关联参数（封装为会话或状态结构）。
- **测试环境隔离**：默认 `cargo test` 用例不得依赖宿主机器上的外部安装目录或个人状态；确需本机真实环境的用例标 `#[ignore = "原因"]`。

## NSH UI 与测试

- **深浅主题单源状态**：直接读取 `nsh_core::prelude::dark()`，通过 `set_color_scheme(...)` 切换，不额外缓存独立的主题 `Signal<bool>`，避免与系统/宿主初始主题脱节。
- **`test_id` 唯一性**：同一棵已挂载视图树（含隐藏页签）中不得重复使用同一个 `test_id`，否则布局测量与交互定位会被后挂载节点覆盖。
- **显示文本保真**：不要向用户可见路径插入零宽字符（如 `\u{200B}`），保证复制与精确文本断言一致。
