//! Curated plugin catalog model, validation, and Extensions page UI.

use crate::launcher::{LauncherEnvironment, button};
use nsh_core::prelude::*;
use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    rc::Rc,
};

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

/// Renders the Extensions page for browsing the curated plugin catalog.
pub(crate) fn section(environment: Rc<dyn LauncherEnvironment>) -> View {
    let state = signal(load_state(environment.as_ref()));
    let plugins_root: Option<PathBuf> = environment.plugins_root();
    let reload_env = environment;

    view()
        .gap_3()
        .child(
            text("扩展")
                .text_size(px(24.0))
                .font_weight(FontWeight::BOLD),
        )
        .child(text(
            "本项目维护的精选插件目录，仅展示已核实条目，不开放任意第三方商店源。",
        ))
        .child(
            view()
                .flex_row()
                .gap_2()
                .child(button("重新加载目录", move || {
                    state.set(load_state(reload_env.as_ref()));
                })),
        )
        .child(dynamic(move || {
            let root = plugins_root.as_deref();
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
                    let mut elements =
                        vec![catalog_list(&catalog, selected_entry.as_ref(), state).into_element()];
                    if let Some(entry) = selected_entry {
                        elements.push(entry_details(root, &entry).into_element());
                    }
                    elements
                }
            }
        }))
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

fn entry_details(plugins_root: Option<&Path>, entry: &PluginEntry) -> View {
    let location = install_location(plugins_root, entry);
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
        .child(text(format!("权限与风险：{}", entry.permissions)))
        .child(text(format!("兼容格式：{}", entry.compatible_format)))
        .child(text(format!("许可证：{}", entry.license)))
        .child(text(format!("说明：{}", entry.notes)))
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
        assert!(entry.permissions.contains("管理员权限"));
        assert!(entry.compatible_format.contains("Launcher_2.exe"));
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
