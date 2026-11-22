use super::{
    ALL, ARTIFACTS, MATERIALS, WEAPONS,
    archive::{Archive, CaptureTime, Inventory},
    store::{self, Header, ImportEffect},
};
use crate::{
    archive_json::file_button,
    launcher::{LauncherEnvironment, button},
    profile::{Profile, VerifiedAccount},
};
use nsh_base::theme;
use nsh_core::{element::input, prelude::*};
use std::{path::PathBuf, rc::Rc};

const PAGE_SIZE: usize = 30;

/// Renders the inventory browser and import controls for the selected UID.
pub(super) fn section(
    environment: Rc<dyn LauncherEnvironment>,
    database: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    match database {
        Ok(path) => warehouse(environment, Rc::new(path), profile, revision),
        Err(error) => view().child(text(error)),
    }
}

fn warehouse(
    environment: Rc<dyn LauncherEnvironment>,
    database: Rc<PathBuf>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
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
        .child(browser(database.clone(), profile, revision))
        .child(mihoyo_sync(environment, database, profile, revision))
        .child(text("导入 GOOD 背包快照").font_weight(FontWeight::BOLD))
        .child(controls.file_input())
        .child(controls.target_options(profile))
        .child(controls.confirmation())
        .child(text(move || controls.message.get()))
}

fn mihoyo_sync(
    environment: Rc<dyn LauncherEnvironment>,
    database: Rc<PathBuf>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    let syncing = signal(false);
    let message: Signal<Option<(String, String)>> = signal(None);
    let owner = Owner::current();
    view()
        .gap_2()
        .child(text("米游社材料同步").font_weight(FontWeight::BOLD))
        .child(dynamic(move || {
            let state = profile.get();
            let matching = state
                .active_uid
                .as_deref()
                .zip(state.account.as_ref())
                .filter(|(uid, account)| *uid == account.uid.as_str())
                .map(|(_, account)| account.clone());
            let Some(account) = matching else {
                return view().child(text(
                    "使用当前已授权米哈游账号绑定的 UID 时，可直接从米游社同步材料。",
                ));
            };
            let busy = syncing.get();
            let environment = environment.clone();
            let database = database.clone();
            let status = message
                .get()
                .filter(|(uid, _)| *uid == account.uid)
                .map(|(_, text)| text)
                .unwrap_or_default();
            view()
                .gap_2()
                .child(text(
                    "通过当前米游社授权的养成计算器同步角色与武器培养材料，自动备份并保留已有武器与圣遗物记录。",
                ))
                .child(if busy {
                    text("正在从米游社同步材料…").into_element()
                } else {
                    button("从米游社同步材料", move || {
                        if syncing.get_untracked() {
                            return;
                        }
                        let account = account.clone();
                        let environment = environment.clone();
                        let database = database.clone();
                        owner.run(move || {
                            run_mihoyo_sync(
                                environment.as_ref(),
                                database,
                                account,
                                profile,
                                revision,
                                syncing,
                                message,
                            );
                        });
                    })
                    .into_element()
                })
                .child((!status.is_empty()).then(|| text(status)))
        }))
}

fn run_mihoyo_sync(
    environment: &dyn LauncherEnvironment,
    database: Rc<PathBuf>,
    account: VerifiedAccount,
    profile: Signal<Profile>,
    revision: Signal<u64>,
    syncing: Signal<bool>,
    message: Signal<Option<(String, String)>>,
) {
    let starting_revision = revision.get_untracked();
    let uid = account.uid.clone();
    syncing.set(true);
    message.set(None);
    let operation = environment.sync_mihoyo_materials(account.clone());
    spawn(async move {
        let result = operation.await;
        syncing.set(false);
        if profile.get_untracked().account.as_ref() != Some(&account) {
            message.set(Some((
                uid,
                "米游社授权已变化，本次材料同步结果未写入。".into(),
            )));
            return;
        }
        if revision.get_untracked() != starting_revision {
            message.set(Some((
                uid,
                "本机历史已变化，本次材料同步结果未写入，请重新同步。".into(),
            )));
            return;
        }
        match result {
            Ok(materials) => {
                let count = materials.len();
                let saved = CaptureTime::from_millis(chrono::Utc::now().timestamp_millis())
                    .and_then(|captured| {
                        store::save_synced_materials(
                            &database,
                            &account,
                            &captured,
                            "MiYouShe-Calculator",
                            materials,
                        )
                    });
                match saved {
                    Ok((effect, backups)) => {
                        revision.update(|value| *value += 1);
                        let paths = backups
                            .iter()
                            .map(|path| path.display().to_string())
                            .collect::<Vec<_>>()
                            .join("、");
                        let note = if effect == ImportEffect::Historical {
                            "（已保存为历史快照，本机已有采集时间更晚的材料记录）"
                        } else {
                            ""
                        };
                        message.set(Some((
                            uid,
                            format!(
                                "已从米游社同步 UID {} 的 {count} 种材料{note}。同步前备份：{paths}",
                                account.uid
                            ),
                        )));
                    }
                    Err(error) => {
                        message.set(Some((uid, format!("保存米游社材料失败：{error}"))));
                    }
                }
            }
            Err(error) => message.set(Some((uid, format!("米游社材料同步失败：{error}")))),
        }
    });
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
            let include_empty = omitted_confirmed.get();
            let timestamp = controls.timestamp;
            let preview = controls.preview;
            let message = controls.message;
            Some(
                view()
                    .gap_2()
                    .child(text(format!("来源：{} · GOOD v{}", archive.source, archive.version)))
                    .child(text(label))
                    .child((archive.coverage != ALL && !include_empty).then(|| {
                        let supplied = categories_label(archive.coverage);
                        text(format!("文件包含：{supplied}。未提供的类别保留本机记录。"))
                    }))
                    .child((!archive.omitted_categories.is_empty()).then(|| {
                        view()
                            .gap_2()
                            .child(text(format!(
                                "文件未列出{}。请核对游戏内库存和扫描范围。",
                                archive.omitted_categories.join("、"),
                            )))
                            .child(button(
                                if include_empty { "撤回空库存确认" } else { "确认缺失类别的库存为空" },
                                move || {
                                    omitted_confirmed.set(!include_empty);
                                    preview.set(None);
                                },
                            ))
                    }))
                    .child(if let Some(time) = &archive.captured_at {
                        text(format!("文件采集时间：{time}")).into_element()
                    } else {
                        view()
                            .gap_2()
                            .child(text("请输入原始采集时间及其时区。Inventory Kamera 或 GGScanner 的原始文件名可提供本地时间。"))
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
        let time = self.timestamp.get_untracked();
        let coverage = if self.omitted_confirmed.get_untracked() {
            ALL
        } else {
            archive.coverage
        };
        let result = CaptureTime::parse(archive.captured_at.as_deref().unwrap_or(&time)).and_then(
            |captured_at| {
                let plan = store::preview(&self.database, uid, &archive, &captured_at, coverage)?;
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
            let coverage = pending.plan.coverage;
            let effect = pending.plan.effect;
            let confirm = controls.clone();
            let counts = Category::ALL
                .into_iter()
                .filter(|category| coverage & *category as u8 != 0)
                .map(|category| match category {
                    Category::Weapons => format!("武器 {} 件", inventory.weapons.len()),
                    Category::Artifacts => format!("圣遗物 {} 件", inventory.artifacts.len()),
                    Category::Materials => format!("材料 {} 种", inventory.materials.len()),
                })
                .collect::<Vec<_>>()
                .join(" · ");
            let current_summary = match pending.plan.replaced.as_slice() {
                [] => None,
                [single] => Some(format!("目标 UID 最新快照：{}", single.captured_at)),
                multiple => Some(format!(
                    "目标 UID 当前相关快照：{}",
                    multiple
                        .iter()
                        .map(|header| {
                            format!(
                                "{}（{}）",
                                header.captured_at,
                                categories_label(header.coverage)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("、")
                )),
            };
            Some(
                view()
                    .gap_2()
                    .child(text(format!("UID {} · {counts}", pending.plan.uid)))
                    .child(text(format!(
                        "待导入采集时间：{}",
                        pending.captured_at.original
                    )))
                    .child(current_summary.map(text))
                    .child(text(match effect {
                        ImportEffect::Latest => "导入后，各类别按采集时间显示最新记录。",
                        ImportEffect::Historical => "这份快照较旧，导入后可在历史中主动选择。",
                        ImportEffect::Duplicate => "这份背包快照已保存。",
                    }))
                    .child((effect != ImportEffect::Duplicate).then(|| {
                        view()
                            .gap_2()
                            .child(text(if coverage == ALL {
                                "请确认三类数据均已完整采集，且扫描时未筛掉装备。"
                            } else {
                                "请确认文件提供的类别已经完整采集。"
                            }))
                            .child(button(
                                if coverage == ALL {
                                    "备份并确认完整导入"
                                } else {
                                    "备份并确认导入所含类别"
                                },
                                move || {
                                    confirm.import(&pending);
                                },
                            ))
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
            Ok(backups) => {
                self.revision.update(|revision| *revision += 1);
                let paths = backups
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("、");
                self.message.set(format!(
                    "已导入 UID {} 的背包快照。导入前备份：{paths}",
                    pending.plan.uid,
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
    let position = signal((Category::Weapons, 0_usize));
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
                        button(label, move || {
                            selected.set(index);
                            position.update(|(_, page)| *page = 0);
                        })
                        .into_element()
                    }))
            })
        }))
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
            let index = selected.get();
            let (category, page) = position.get();
            let bit = category as u8;
            let header = if index == 0 {
                let Some(header) = history.iter().find(|header| header.coverage & bit != 0) else {
                    return view().child(text(format!("{}记录尚未采集", category.label())));
                };
                header
            } else {
                &history[index]
            };
            let content = if header.coverage & bit == 0 {
                view().child(text(format!("此旧快照未包含{}记录", category.label())))
            } else {
                match store::read(&database, &uid, header.id) {
                    Ok(inventory) => inventory_rows(&inventory, category, page, position),
                    Err(error) => view().child(text(error)),
                }
            };
            view()
                .gap_2()
                .child(text(match (index, header.coverage) {
                    (0, ALL) => "最新完整快照",
                    (0, _) => "最新分类快照",
                    _ => "正在查看旧快照",
                }))
                .child(text(format!("采集时间：{}", header.captured_at)))
                .child(text(format!(
                    "来源：{} · GOOD v{}",
                    header.source, header.version
                )))
                .child(content)
        }))
}

#[derive(Clone, Copy, PartialEq)]
enum Category {
    Weapons = WEAPONS as isize,
    Artifacts = ARTIFACTS as isize,
    Materials = MATERIALS as isize,
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

fn categories_label(coverage: u8) -> String {
    Category::ALL
        .into_iter()
        .filter(|category| coverage & *category as u8 != 0)
        .map(Category::label)
        .collect::<Vec<_>>()
        .join("、")
}

fn inventory_rows(
    inventory: &Inventory,
    category: Category,
    page: usize,
    position: Signal<(Category, usize)>,
) -> View {
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
}
