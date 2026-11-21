use super::{
    archive::{Archive, CaptureTime, Inventory},
    store::{self, Header, ImportEffect},
};
use crate::{archive_json::file_button, launcher::button, profile::Profile};
use nsh_base::theme;
use nsh_core::{element::input, prelude::*};
use std::{path::PathBuf, rc::Rc};

const PAGE_SIZE: usize = 30;

/// Adds the expandable inventory browser and import controls to the UID page.
pub(super) fn section(
    database: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    let expanded = signal(false);
    view()
        .gap_2()
        .child(button("背包仓库", move || {
            expanded.update(|open| *open = !*open);
        }))
        .child(dynamic(move || {
            expanded.get().then(|| match &database {
                Ok(path) => warehouse(Rc::new(path.clone()), profile, revision),
                Err(error) => view().child(text(error.clone())),
            })
        }))
}

fn warehouse(database: Rc<PathBuf>, profile: Signal<Profile>, revision: Signal<u64>) -> View {
    let controls = ImportControls {
        database: database.clone(),
        path: signal(String::new()),
        timestamp: signal(String::new()),
        archive: signal_with(None, SignalEquals::Never),
        omitted_confirmed: signal(false),
        preview: signal_with(None, SignalEquals::Never),
        message: signal(String::new()),
        revision,
    };
    view()
        .gap_2()
        .child(browser(database, profile, revision))
        .child(text("导入 GOOD 背包快照").font_weight(FontWeight::BOLD))
        .child(controls.file_input())
        .child(controls.target_options(profile))
        .child(controls.confirmation())
        .child(text(move || controls.message.get()))
}

#[derive(Clone)]
struct ImportControls {
    database: Rc<PathBuf>,
    path: Signal<String>,
    timestamp: Signal<String>,
    archive: Signal<Option<Rc<Archive>>>,
    omitted_confirmed: Signal<bool>,
    preview: Signal<Option<Rc<PendingImport>>>,
    message: Signal<String>,
    revision: Signal<u64>,
}

struct PendingImport {
    archive: Rc<Archive>,
    captured_at: CaptureTime,
    plan: store::Preview,
    history_revision: u64,
}

impl ImportControls {
    fn file_input(&self) -> View {
        let path = self.path;
        let edit = self.clone();
        let pick = self.clone();
        let load = self.clone();
        view()
            .gap_2()
            .child(
                input()
                    .test_id("inventory-file")
                    .label("GOOD JSON 文件路径")
                    .placeholder("GOOD JSON 文件路径")
                    .h_10()
                    .px_3()
                    .bg(move || theme().secondary)
                    .value(move || path.get())
                    .on_change(move |value: &str| {
                        path.set(value.to_owned());
                        edit.clear_file();
                    }),
            )
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .child(file_button("GOOD", move |result| match result {
                        Ok(path) => {
                            pick.path.set(path.to_string_lossy().into_owned());
                            pick.load();
                        }
                        Err(error) => pick.message.set(error),
                    }))
                    .child(button("读取 GOOD 文件", move || load.load())),
            )
    }

    fn clear_file(&self) {
        self.preview.set(None);
        self.archive.set(None);
        self.omitted_confirmed.set(false);
        self.message.set(String::new());
    }

    fn load(&self) {
        self.clear_file();
        let path = self.path.get_untracked();
        let path = path.trim().trim_matches('"');
        if path.is_empty() {
            self.message.set("请先选择 GOOD JSON 文件".into());
            return;
        }
        let path = std::path::Path::new(path);
        match Archive::read(path) {
            Ok(archive) => {
                self.timestamp.set(archive.suggested_time(path));
                self.archive.set(Some(Rc::new(archive)));
            }
            Err(error) => self.message.set(error),
        }
    }

    fn target_options(&self, profile: Signal<Profile>) -> impl IntoElement {
        let controls = self.clone();
        dynamic(move || {
            let archive = controls.archive.get()?;
            let targets = profile.get().uids;
            let label = if let Some(uid) = &archive.uid {
                if targets.contains(uid) {
                    format!("文件声明游戏 UID：{uid}")
                } else {
                    format!("请先创建文件声明的 UID {uid} 数据空间")
                }
            } else if targets.is_empty() {
                "请先创建导入目标的 UID 数据空间".to_owned()
            } else {
                "文件未声明 UID，请明确选择导入目标".to_owned()
            };
            let omitted_confirmed = controls.omitted_confirmed;
            let can_preview = archive.omitted_categories.is_empty() || omitted_confirmed.get();
            let timestamp = controls.timestamp;
            let preview = controls.preview;
            let message = controls.message;
            Some(
                view()
                    .gap_2()
                    .child(text(format!("来源：{} · GOOD v{}", archive.source, archive.version)))
                    .child(text(label))
                    .child((!archive.omitted_categories.is_empty()).then(|| {
                        view()
                            .gap_2()
                            .child(text(format!(
                                "文件未列出{}。请核对游戏内库存和扫描范围。",
                                archive.omitted_categories.join("、"),
                            )))
                            .child(button(
                                if can_preview { "撤回空库存确认" } else { "确认缺失类别的库存为空" },
                                move || {
                                    omitted_confirmed.set(!can_preview);
                                    preview.set(None);
                                },
                            ))
                    }))
                    .child(if let Some(time) = &archive.captured_at {
                        text(format!("文件采集时间：{time}")).into_element()
                    } else {
                        view()
                            .gap_2()
                            .child(text("请输入原始采集时间及其时区。Inventory Kamera 的原始文件名可提供本地时间。"))
                            .child(
                                input()
                                    .test_id("inventory-captured-at")
                                    .label("原始采集时间（含时区）")
                                    .placeholder("2026-10-09T08:00:00+08:00")
                                    .h_10()
                                    .px_3()
                                    .bg(move || theme().secondary)
                                    .value(move || timestamp.get())
                                    .on_change(move |value: &str| {
                                        timestamp.set(value.to_owned());
                                        preview.set(None);
                                        message.set(String::new());
                                    }),
                            )
                            .into_element()
                    })
                    .child(
                        view().flex_row().flex_wrap().gap_2().children(
                            targets
                                .into_iter()
                                .filter(|_| can_preview)
                                .filter(|uid| archive.uid.as_ref().is_none_or(|expected| uid == expected))
                                .map(|uid| {
                                    let controls = controls.clone();
                                    let archive = archive.clone();
                                    button(format!("预览背包导入至 UID {uid}"), move || {
                                        controls.preview_import(&uid, archive.clone());
                                    }).into_element()
                                }),
                        ),
                    ),
            )
        })
    }

    fn preview_import(&self, uid: &str, archive: Rc<Archive>) {
        self.preview.set(None);
        if !archive.omitted_categories.is_empty() && !self.omitted_confirmed.get_untracked() {
            return;
        }
        let time = self.timestamp.get_untracked();
        let result = CaptureTime::parse(archive.captured_at.as_deref().unwrap_or(&time)).and_then(
            |captured_at| {
                let plan = store::preview(&self.database, uid, &archive, &captured_at)?;
                Ok(PendingImport {
                    archive,
                    captured_at,
                    plan,
                    history_revision: self.revision.get_untracked(),
                })
            },
        );
        match result {
            Ok(pending) => {
                self.message.set(String::new());
                self.preview.set(Some(Rc::new(pending)));
            }
            Err(error) => self.message.set(error),
        }
    }

    fn confirmation(&self) -> impl IntoElement {
        let controls = self.clone();
        dynamic(move || {
            let pending = controls.preview.get()?;
            if pending.history_revision != controls.revision.get() {
                return Some(view().child(text("本机历史已变化，请重新预览")));
            }
            let inventory = &pending.archive.inventory;
            let effect = pending.plan.effect;
            let confirm = controls.clone();
            Some(
                view()
                    .gap_2()
                    .child(text(format!(
                        "UID {} · 武器 {} 件 · 圣遗物 {} 件 · 材料 {} 种",
                        pending.plan.uid,
                        inventory.weapons.len(),
                        inventory.artifacts.len(),
                        inventory.materials.len(),
                    )))
                    .child(text(format!(
                        "待导入采集时间：{}",
                        pending.captured_at.original
                    )))
                    .child(
                        pending.plan.current.as_ref().map(|current| {
                            text(format!("目标 UID 最新快照：{}", current.captured_at))
                        }),
                    )
                    .child(text(match effect {
                        ImportEffect::Latest => "导入后默认显示这份完整快照。",
                        ImportEffect::Historical => "这份快照较旧，导入后可在历史中主动选择。",
                        ImportEffect::Duplicate => "这份背包快照已保存。",
                    }))
                    .child((effect != ImportEffect::Duplicate).then(|| {
                        view()
                            .gap_2()
                            .child(text("请确认三类数据均已完整采集，且扫描时未筛掉装备。"))
                            .child(button("备份并确认完整导入", move || {
                                confirm.import(&pending);
                            }))
                    })),
            )
        })
    }

    fn import(&self, pending: &Rc<PendingImport>) {
        if pending.history_revision != self.revision.get_untracked()
            || !self
                .preview
                .get_untracked()
                .is_some_and(|current| Rc::ptr_eq(&current, pending))
        {
            return;
        }
        self.preview.set(None);
        match store::import(
            &self.database,
            &pending.archive,
            &pending.captured_at,
            &pending.plan,
        ) {
            Ok(backup) => {
                self.revision.update(|revision| *revision += 1);
                self.message.set(format!(
                    "已导入 UID {} 的背包快照。导入前备份：{}",
                    pending.plan.uid,
                    backup.display(),
                ));
            }
            Err(error) => self.message.set(format!("导入失败：{error}")),
        }
    }
}

fn browser(
    database: Rc<PathBuf>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> impl IntoElement {
    dynamic(move || {
        revision.get();
        let Some(uid) = profile.get().active_uid else {
            return view().child(text("请在角色资料中选择游戏 UID"));
        };
        let heading = view().gap_2().child(text(format!("UID {uid} 的背包仓库")));
        match store::list(&database, &uid) {
            Ok(history) if history.is_empty() => heading.child(text("暂无背包快照")),
            Ok(history) => heading.child(snapshot_browser(database.clone(), uid, history)),
            Err(error) => heading.child(text(error)),
        }
    })
}

fn snapshot_browser(database: Rc<PathBuf>, uid: String, history: Vec<Header>) -> View {
    let selected = signal(0);
    let history_open = signal(false);
    let history = Rc::new(history);
    let options = history.clone();
    view()
        .gap_2()
        .child((history.len() > 1).then(|| {
            button("背包快照历史", move || {
                history_open.update(|open| *open = !*open);
            })
        }))
        .child(dynamic(move || {
            history_open.get().then(|| {
                view()
                    .gap_2()
                    .children(options.iter().enumerate().map(|(index, header)| {
                        let label = if index == 0 {
                            "查看最新背包快照".to_owned()
                        } else {
                            format!("查看旧快照：{}", header.captured_at)
                        };
                        button(label, move || selected.set(index)).into_element()
                    }))
            })
        }))
        .child(dynamic(move || {
            let index = selected.get();
            let header = &history[index];
            view()
                .gap_2()
                .child(text(if index == 0 {
                    "最新完整快照"
                } else {
                    "正在查看旧快照"
                }))
                .child(text(format!("采集时间：{}", header.captured_at)))
                .child(text(format!(
                    "来源：{} · GOOD v{}",
                    header.source, header.version
                )))
                .child(match store::read(&database, &uid, header.id) {
                    Ok(inventory) => inventory_rows(inventory),
                    Err(error) => view().child(text(error)),
                })
        }))
}

#[derive(Clone, Copy, PartialEq)]
enum Category {
    Weapons,
    Artifacts,
    Materials,
}

impl Category {
    const ALL: [Self; 3] = [Self::Weapons, Self::Artifacts, Self::Materials];

    fn label(self) -> &'static str {
        match self {
            Self::Weapons => "武器",
            Self::Artifacts => "圣遗物",
            Self::Materials => "材料",
        }
    }
}

fn inventory_rows(inventory: Inventory) -> View {
    let position = signal((Category::Weapons, 0_usize));
    let inventory = Rc::new(inventory);
    view()
        .gap_2()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .children(Category::ALL.into_iter().map(|category| {
                    button(category.label(), move || position.set((category, 0))).into_element()
                })),
        )
        .child(dynamic(move || {
            let (category, page) = position.get();
            let start = page * PAGE_SIZE;
            let label = category.label();
            let (total, rows): (_, Vec<_>) = match category {
                Category::Weapons => (
                    inventory.weapons.len(),
                    inventory
                        .weapons
                        .iter()
                        .skip(start)
                        .take(PAGE_SIZE)
                        .map(super::archive::Weapon::label)
                        .collect(),
                ),
                Category::Artifacts => (
                    inventory.artifacts.len(),
                    inventory
                        .artifacts
                        .iter()
                        .skip(start)
                        .take(PAGE_SIZE)
                        .map(super::archive::Artifact::label)
                        .collect(),
                ),
                Category::Materials => (
                    inventory.materials.len(),
                    inventory
                        .materials
                        .iter()
                        .skip(start)
                        .take(PAGE_SIZE)
                        .map(|(key, amount)| format!("{key} · 数量 {amount}"))
                        .collect(),
                ),
            };
            if total == 0 {
                return view().child(text(format!("此快照无{label}记录")));
            }
            view()
                .gap_2()
                .child(text(format!(
                    "{label} · 共 {total} 项 · 第 {}/{} 页",
                    page + 1,
                    total.div_ceil(PAGE_SIZE)
                )))
                .children(rows.into_iter().map(|row| text(row).into_element()))
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .child((page > 0).then(|| {
                            button("上一页背包", move || {
                                position.set((category, page - 1));
                            })
                        }))
                        .child((start + PAGE_SIZE < total).then(|| {
                            button("下一页背包", move || {
                                position.set((category, page + 1));
                            })
                        })),
                )
        }))
}
