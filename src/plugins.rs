//! Curated plugin catalog model, validation, and Extensions page UI.

mod store;

use crate::launcher::{LauncherEnvironment, button};
use nsh_core::element::input;
use nsh_core::prelude::*;
use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    rc::Rc,
};
use store::{InstallStatus, UpdateOutcome};

const SUPPORTED_CATALOG_VERSION: u32 = 1;
const BUNDLED_CATALOG_JSON: &str = include_str!("../assets/plugin-catalog.json");

/// Represents the curated plugin catalog maintained by Genshin-Go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginCatalog {
    entries: Vec<PluginEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct PluginEntry {
    id: String,
    name: String,
    kind: String,
    version: String,
    commit: String,
    source_url: String,
    install_subpath: String,
    default_local_source: String,
    required_files: Vec<String>,
    permissions: String,
    compatible_format: String,
    license: String,
    notes: String,
}

#[derive(Deserialize)]
struct RawCatalog {
    version: u32,
    entries: Vec<PluginEntry>,
}

fn parse_catalog(raw: &str) -> Result<PluginCatalog, String> {
    let parsed: RawCatalog =
        serde_json::from_str(raw).map_err(|error| format!("插件目录 JSON 格式无效：{error}"))?;
    if parsed.version != SUPPORTED_CATALOG_VERSION {
        return Err(format!("不支持的插件目录版本：{}", parsed.version));
    }
    if parsed.entries.is_empty() {
        return Err("精选插件目录为空".into());
    }

    let mut seen_ids = HashSet::new();
    let mut entries = Vec::with_capacity(parsed.entries.len());
    for raw_entry in parsed.entries {
        let entry = validate_entry(raw_entry)?;
        if !seen_ids.insert(entry.id.clone()) {
            return Err(format!("插件目录存在重复标识：{}", entry.id));
        }
        entries.push(entry);
    }
    Ok(PluginCatalog { entries })
}

/// Loads the curated plugin catalog bundled with the launcher.
pub fn bundled_catalog() -> Result<PluginCatalog, String> {
    parse_catalog(BUNDLED_CATALOG_JSON)
}

fn install_location(plugins_root: Option<&Path>, entry: &PluginEntry) -> String {
    match plugins_root {
        Some(root) => root.join(&entry.install_subpath).display().to_string(),
        None => format!(
            r"%LOCALAPPDATA%\Genshin-Go\plugins\{}",
            entry.install_subpath
        ),
    }
}

fn validate_entry(mut entry: PluginEntry) -> Result<PluginEntry, String> {
    entry.id = require_non_empty("id", &entry.id)?;
    entry.name = require_non_empty("name", &entry.name)?;
    entry.kind = require_non_empty("kind", &entry.kind)?;
    entry.version = require_non_empty("version", &entry.version)?;
    entry.commit = require_non_empty("commit", &entry.commit)?;
    entry.source_url = require_non_empty("source_url", &entry.source_url)?;
    if !entry.source_url.starts_with("https://") {
        return Err(format!("插件 {} 的来源地址必须使用 https://", entry.id));
    }
    entry.install_subpath = require_non_empty("install_subpath", &entry.install_subpath)?;
    validate_install_subpath(&entry.id, &entry.install_subpath)?;
    entry.default_local_source =
        require_non_empty("default_local_source", &entry.default_local_source)?;
    if entry.required_files.is_empty() {
        return Err(format!("插件 {} 缺少必需文件列表", entry.id));
    }
    for file in &mut entry.required_files {
        *file = require_non_empty("required_files", file)?;
        store::validate_file_name(file)?;
    }
    entry.permissions = require_non_empty("permissions", &entry.permissions)?;
    entry.compatible_format = require_non_empty("compatible_format", &entry.compatible_format)?;
    entry.license = require_non_empty("license", &entry.license)?;
    entry.notes = require_non_empty("notes", &entry.notes)?;
    Ok(entry)
}

fn require_non_empty(field: &str, value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("插件目录缺少必填字段：{field}"));
    }
    Ok(trimmed.to_owned())
}

fn validate_install_subpath(id: &str, subpath: &str) -> Result<(), String> {
    let mut components = Path::new(subpath).components();
    let valid = !subpath.contains('\\')
        && !subpath.contains(':')
        && components.clone().next().is_some()
        && components.all(|component| matches!(component, Component::Normal(_)));
    if !valid {
        return Err(format!("插件 {id} 的安装子路径不合法：{subpath}"));
    }
    Ok(())
}

#[derive(Clone, PartialEq, Eq)]
enum CatalogState {
    Loaded {
        catalog: PluginCatalog,
        selected_id: Option<String>,
    },
    Failed(String),
}

fn load_state(environment: &dyn LauncherEnvironment) -> CatalogState {
    match environment.load_plugin_catalog() {
        Ok(catalog) => {
            let selected_id = catalog.entries.first().map(|entry| entry.id.clone());
            CatalogState::Loaded {
                catalog,
                selected_id,
            }
        }
        Err(error) => CatalogState::Failed(error),
    }
}

/// Renders the Extensions page for browsing and managing the curated plugin catalog.
pub(crate) fn section(environment: Rc<dyn LauncherEnvironment>) -> View {
    let state = signal(load_state(environment.as_ref()));
    let plugins_root: Option<PathBuf> = environment.plugins_root();
    let source_input = signal(initial_source_path(&state.get_untracked()));
    let action_message = signal(String::new());
    let status_revision = signal(0_u64);
    let reload_env = environment;

    view()
        .gap_3()
        .child(text(
            "本项目维护的精选插件目录，仅展示已核实条目，不开放任意第三方商店源。",
        ))
        .child(
            view()
                .flex_row()
                .gap_2()
                .child(button("重新加载目录", move || {
                    let next = load_state(reload_env.as_ref());
                    if source_input.get_untracked().is_empty() {
                        source_input.set(initial_source_path(&next));
                    }
                    state.set(next);
                    status_revision.update(|rev| *rev += 1);
                })),
        )
        .child(dynamic(move || {
            let _ = status_revision.get();
            let root = plugins_root.clone();
            match state.get() {
                CatalogState::Failed(error) => vec![load_failure_card(&error).into_element()],
                CatalogState::Loaded {
                    catalog,
                    selected_id,
                } => {
                    let selected_entry = selected_id
                        .as_deref()
                        .and_then(|id| catalog.entries.iter().find(|entry| entry.id == id))
                        .or_else(|| catalog.entries.first())
                        .cloned();
                    let mut elements = vec![
                        catalog_list(&catalog, selected_entry.as_ref(), state, source_input)
                            .into_element(),
                    ];
                    if let Some(entry) = selected_entry {
                        elements.push(
                            entry_details(
                                root,
                                &entry,
                                source_input,
                                action_message,
                                status_revision,
                            )
                            .into_element(),
                        );
                    }
                    elements
                }
            }
        }))
}

fn initial_source_path(state: &CatalogState) -> String {
    match state {
        CatalogState::Loaded { catalog, .. } => catalog
            .entries
            .first()
            .map(|entry| entry.default_local_source.clone())
            .unwrap_or_default(),
        CatalogState::Failed(_) => String::new(),
    }
}

fn load_failure_card(error: &str) -> View {
    view()
        .gap_2()
        .p_3()
        .rounded(px(8.0))
        .bg(AdaptiveColor::rgb(0x00f7_e7e4, 0x003f_2320))
        .child(text(format!("目录加载失败：{error}")).font_weight(FontWeight::BOLD))
        .child(text(
            "未能读取精选插件目录，请稍后点击「重新加载目录」重试。",
        ))
}

fn catalog_list(
    catalog: &PluginCatalog,
    selected_entry: Option<&PluginEntry>,
    state: Signal<CatalogState>,
    source_input: Signal<String>,
) -> View {
    view()
        .gap_2()
        .child(
            text(format!("精选目录（共 {} 项）", catalog.entries.len()))
                .text_size(px(16.0))
                .font_weight(FontWeight::BOLD),
        )
        .children(catalog.entries.iter().map(|entry| {
            let entry_id = entry.id.clone();
            let default_source = entry.default_local_source.clone();
            let is_selected = selected_entry.is_some_and(|selected| selected.id == entry.id);
            let label = format!("{} · {} · {}", entry.name, entry.kind, entry.version);
            view()
                .px_3()
                .py_2()
                .rounded(px(8.0))
                .role(AccessibilityRole::Button)
                .accessibility_state(move || AccessibilityState {
                    selected: is_selected,
                    ..Default::default()
                })
                .bg(if is_selected {
                    AdaptiveColor::rgb(0x0021_5b49, 0x0021_5b49)
                } else {
                    AdaptiveColor::rgb(0x00e0_e8da, 0x002a_473d)
                })
                .text_color(if is_selected {
                    AdaptiveColor::rgb(0x00ff_ffff, 0x00ff_ffff)
                } else {
                    AdaptiveColor::rgb(0x0020_3832, 0x00f4_f5ef)
                })
                .on_press(move || {
                    source_input.set(default_source.clone());
                    state.update(|current| {
                        if let CatalogState::Loaded { selected_id, .. } = current {
                            *selected_id = Some(entry_id.clone());
                        }
                    });
                })
                .child(text(label))
                .into_element()
        }))
}

fn entry_details(
    plugins_root: Option<PathBuf>,
    entry: &PluginEntry,
    source_input: Signal<String>,
    action_message: Signal<String>,
    status_revision: Signal<u64>,
) -> View {
    let location = install_location(plugins_root.as_deref(), entry);
    let inspect_result = plugins_root.as_deref().map(|root| {
        store::inspect(
            root,
            &entry.install_subpath,
            &entry.id,
            &entry.required_files,
        )
    });

    let (status_line, version_line) = status_summary(inspect_result.as_ref());
    let controls = management_controls(
        plugins_root,
        entry,
        inspect_result.as_ref(),
        source_input,
        action_message,
        status_revision,
    );

    view()
        .gap_2()
        .p_3()
        .rounded(px(8.0))
        .bg(AdaptiveColor::rgb(0x00e5_ecdb, 0x0025_3b32))
        .child(
            text(format!("条目详情：{}", entry.name))
                .text_size(px(18.0))
                .font_weight(FontWeight::BOLD),
        )
        .child(text(format!("类型：{}", entry.kind)))
        .child(text(format!("版本：{}", entry.version)))
        .child(text(format!("核实提交：{}", entry.commit)))
        .child(text(format!("来源：{}", entry.source_url)))
        .child(text(format!("安装位置：{location}")))
        .child(text(status_line).font_weight(FontWeight::BOLD))
        .child(text(version_line))
        .child(text(format!("权限与风险：{}", entry.permissions)))
        .child(text(format!("兼容格式：{}", entry.compatible_format)))
        .child(text(format!("许可证：{}", entry.license)))
        .child(text(format!("说明：{}", entry.notes)))
        .child(controls)
        .child(text(move || action_message.get()))
}

fn format_install_record(record: &store::InstalledPlugin) -> String {
    format!(
        "安装时目录版本：{}（安装时内容摘要：{}）",
        record.version,
        record.short_hash()
    )
}

fn status_summary(inspect_result: Option<&Result<InstallStatus, String>>) -> (String, String) {
    match inspect_result {
        None => (
            "安装状态：当前环境不可管理本机插件目录".into(),
            "安装时目录版本：未安装".into(),
        ),
        Some(Err(error)) => (
            format!("安装状态：校验失败（{error}）"),
            "安装时目录版本：未知".into(),
        ),
        Some(Ok(InstallStatus::NotInstalled)) => {
            ("安装状态：未安装".into(), "安装时目录版本：未安装".into())
        }
        Some(Ok(InstallStatus::Installed(record))) => {
            ("安装状态：已安装".into(), format_install_record(record))
        }
        Some(Ok(InstallStatus::Corrupted { record, reason })) => (
            format!("安装状态：损坏（{reason}）"),
            format_install_record(record),
        ),
    }
}

fn management_controls(
    plugins_root: Option<PathBuf>,
    entry: &PluginEntry,
    inspect_result: Option<&Result<InstallStatus, String>>,
    source_input: Signal<String>,
    action_message: Signal<String>,
    status_revision: Signal<u64>,
) -> View {
    let Some(root) = plugins_root else {
        return view();
    };
    let is_installed = matches!(inspect_result, Some(Ok(InstallStatus::Installed(_))));
    let is_corrupted = matches!(inspect_result, Some(Ok(InstallStatus::Corrupted { .. })));
    let can_uninstall = !matches!(inspect_result, Some(Ok(InstallStatus::NotInstalled)));

    let mut row = view().flex_row().flex_wrap().gap_2();
    if is_installed {
        row = row.child(update_button(
            root.clone(),
            entry.clone(),
            source_input,
            action_message,
            status_revision,
        ));
    } else {
        row = row.child(install_button(
            root.clone(),
            entry.clone(),
            is_corrupted,
            source_input,
            action_message,
            status_revision,
        ));
    }
    if can_uninstall {
        row = row.child(uninstall_button(
            root,
            entry.clone(),
            action_message,
            status_revision,
        ));
    }

    view()
        .gap_2()
        .child(text("本地插件来源目录（手动安装与更新）："))
        .child(
            input()
                .test_id("plugin-source-dir")
                .label("本地插件来源目录")
                .h_10()
                .px_3()
                .bg(AdaptiveColor::rgb(0x00e0_e8da, 0x002a_473d))
                .placeholder(r"例如 C:\softwares\FufuLauncher\Plugins\FuFuPlugin")
                .value(move || source_input.get())
                .on_change(move |value: &str| source_input.set(value.to_owned())),
        )
        .child(row)
}

fn install_button(
    root: PathBuf,
    entry: PluginEntry,
    is_corrupted: bool,
    source_input: Signal<String>,
    action_message: Signal<String>,
    status_revision: Signal<u64>,
) -> impl IntoElement {
    let label = if is_corrupted {
        "重新安装"
    } else {
        "安装插件"
    };
    button(label, move || {
        let source_text = source_input.get_untracked();
        let source = Path::new(source_text.trim().trim_matches('"'));
        match store::install(
            &root,
            &entry.install_subpath,
            &entry.id,
            &entry.version,
            source,
            &entry.required_files,
        ) {
            Ok(record) => {
                action_message.set(format!(
                    "已安装 {}（内容校验：{}）",
                    entry.name,
                    record.short_hash()
                ));
                status_revision.update(|rev| *rev += 1);
            }
            Err(error) => action_message.set(format!("安装失败：{error}")),
        }
    })
}

fn update_button(
    root: PathBuf,
    entry: PluginEntry,
    source_input: Signal<String>,
    action_message: Signal<String>,
    status_revision: Signal<u64>,
) -> impl IntoElement {
    button("检查并更新", move || {
        let source_text = source_input.get_untracked();
        let source = Path::new(source_text.trim().trim_matches('"'));
        match store::update(
            &root,
            &entry.install_subpath,
            &entry.id,
            &entry.version,
            source,
            &entry.required_files,
        ) {
            Ok(UpdateOutcome::UpToDate(record)) => {
                action_message.set(format!(
                    "与所选来源的内容相同，无需更新（内容摘要：{}）",
                    record.short_hash()
                ));
            }
            Ok(UpdateOutcome::Updated(record)) => {
                action_message.set(format!(
                    "已更新 {}（内容校验：{}）",
                    entry.name,
                    record.short_hash()
                ));
                status_revision.update(|rev| *rev += 1);
            }
            Err(error) => action_message.set(format!("更新失败：{error}")),
        }
    })
}

fn uninstall_button(
    root: PathBuf,
    entry: PluginEntry,
    action_message: Signal<String>,
    status_revision: Signal<u64>,
) -> impl IntoElement {
    button("卸载插件", move || {
        match store::uninstall(
            &root,
            &entry.install_subpath,
            &entry.id,
            &entry.required_files,
        ) {
            Ok(()) => {
                action_message.set(format!("已卸载 {}", entry.name));
                status_revision.update(|rev| *rev += 1);
            }
            Err(error) => action_message.set(format!("卸载失败：{error}")),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_contains_only_confirmed_fufu_unlocker_island() -> Result<(), String> {
        let catalog = bundled_catalog()?;
        assert_eq!(catalog.entries.len(), 1);
        let entry = &catalog.entries[0];
        assert_eq!(entry.id, "fufu-unlocker-island");
        assert_eq!(entry.name, "FufuLauncher.UnlockerIsland");
        assert_eq!(entry.kind, "游戏注入模块");
        assert_eq!(
            entry.source_url,
            "https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland"
        );
        assert_eq!(entry.commit, "1830f9d770ecb9c895ba98b9a933973eabe36e7a");
        assert_eq!(entry.install_subpath, "fufu-unlocker-island");
        assert_eq!(
            entry.default_local_source,
            r"C:\softwares\FufuLauncher\Plugins\FuFuPlugin"
        );
        assert_eq!(
            entry.required_files,
            vec![
                "FufuLauncher.UnlockerIsland.dll".to_owned(),
                "config.ini".to_owned()
            ]
        );
        assert!(entry.permissions.contains("管理员权限"));
        assert!(
            entry
                .compatible_format
                .contains("FufuLauncher.UnlockerIsland.dll")
        );
        assert!(entry.license.contains("AGPL-3.0"));
        Ok(())
    }

    #[test]
    fn rejects_invalid_json_or_unsupported_version() {
        assert!(parse_catalog("{invalid").is_err());
        let wrong_version = r#"{"version": 2, "entries": []}"#;
        assert_eq!(
            parse_catalog(wrong_version),
            Err("不支持的插件目录版本：2".into())
        );
        let empty = r#"{"version": 1, "entries": []}"#;
        assert_eq!(parse_catalog(empty), Err("精选插件目录为空".into()));
    }

    #[test]
    fn rejects_non_https_source_and_path_traversal() {
        let non_https = BUNDLED_CATALOG_JSON.replace("https://github.com", "http://github.com");
        assert!(matches!(
            parse_catalog(&non_https),
            Err(error) if error.contains("必须使用 https://")
        ));

        let traversal =
            BUNDLED_CATALOG_JSON.replace("\"fufu-unlocker-island\"", "\"../outside-plugins\"");
        assert!(matches!(
            parse_catalog(&traversal),
            Err(error) if error.contains("安装子路径不合法")
        ));
    }

    #[test]
    fn computes_install_location_from_plugins_root_or_fallback() -> Result<(), String> {
        let catalog = bundled_catalog()?;
        let entry = &catalog.entries[0];
        assert_eq!(
            install_location(Some(Path::new(r"C:\Genshin-Go\plugins")), entry),
            r"C:\Genshin-Go\plugins\fufu-unlocker-island"
        );
        assert_eq!(
            install_location(None, entry),
            r"%LOCALAPPDATA%\Genshin-Go\plugins\fufu-unlocker-island"
        );
        Ok(())
    }
}
