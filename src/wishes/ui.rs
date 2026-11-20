use super::archive::{Archive, Group};
use super::store::{self, ImportPlan, PAGE_SIZE};
use crate::launcher::button;
use crate::profile::Profile;
use nsh_base::theme;
use nsh_core::element::{input, open_url, pick_paths};
use nsh_core::file_dialog::{FileFilter, OpenDialog};
use nsh_core::prelude::*;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone)]
struct ImportControls {
    database: Rc<PathBuf>,
    input: Signal<String>,
    archive: Signal<Option<Rc<Archive>>>,
    group: Signal<usize>,
    preview: Signal<Option<Rc<ImportPlan>>>,
    message: Signal<String>,
    revision: Signal<u64>,
}

impl ImportControls {
    fn set_input(&self, value: String) {
        self.input.set(value);
        self.archive.set(None);
        self.preview.set(None);
        self.message.set(String::new());
    }

    fn load(&self) {
        self.archive.set(None);
        self.preview.set(None);
        let input = self.input.get_untracked();
        let path = input.trim().trim_matches('"');
        if path.is_empty() {
            self.message.set("请先选择 UIGF JSON 文件".into());
            return;
        }
        match Archive::read(&PathBuf::from(path)) {
            Ok(archive) => {
                self.group.set(0);
                self.archive.set(Some(Rc::new(archive)));
                self.message.set(String::new());
            }
            Err(error) => self.message.set(error),
        }
    }

    fn preview(&self, uid: &str, group: &Group) {
        self.preview.set(None);
        match store::preview(&self.database, uid, group) {
            Ok(preview) => {
                self.preview.set(Some(Rc::new(preview)));
                self.message.set(String::new());
            }
            Err(error) => self.message.set(error),
        }
    }

    fn import(&self, archive: &Archive, index: usize, preview: &Rc<ImportPlan>) {
        if !self
            .preview
            .get_untracked()
            .is_some_and(|current| Rc::ptr_eq(&current, preview))
        {
            return;
        }
        self.preview.set(None);
        match store::import(&self.database, &archive.groups[index], preview) {
            Ok(backup) => {
                self.revision.update(|revision| *revision += 1);
                self.message.set(format!(
                    "已导入 UID {} 的 {} 条祈愿记录。导入前备份：{}",
                    preview.uid,
                    preview.additions.len(),
                    backup.display()
                ));
            }
            Err(error) => self.message.set(format!("导入失败：{error}")),
        }
    }

    fn file_input(&self) -> impl IntoElement {
        let value = self.input;
        let edit = self.clone();
        let browse = self.clone();
        let load = self.clone();
        let owner = Owner::current();
        view()
            .gap_2()
            .child(
                input()
                    .test_id("uigf-file")
                    .label("UIGF JSON 文件路径")
                    .placeholder("UIGF JSON 文件路径")
                    .h_10()
                    .px_3()
                    .bg(move || theme().secondary)
                    .value(move || value.get())
                    .on_change(move |value: &str| edit.set_input(value.to_owned())),
            )
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .child(button("选择 UIGF 文件", move || {
                        let browse = browse.clone();
                        owner.run(move || {
                            let request = pick_paths(
                                OpenDialog::files()
                                    .title("选择 UIGF JSON 文件")
                                    .filter(FileFilter::new("UIGF JSON", ["json"])),
                            );
                            spawn(async move {
                                match request.await {
                                    Ok(Some(paths)) => {
                                        if let Some(path) = paths.first() {
                                            browse.set_input(path.to_string_lossy().into_owned());
                                            browse.load();
                                        }
                                    }
                                    Ok(None) => (),
                                    Err(error) => browse.message.set(error),
                                }
                            });
                        });
                    }))
                    .child(button("读取 UIGF 文件", move || load.load())),
            )
    }

    fn loaded(&self, profile: Signal<Profile>) -> impl IntoElement {
        let controls = self.clone();
        dynamic(move || {
            controls.archive.get().map(|archive| {
                let index = controls.group.get();
                let mut content = view().gap_2();
                if !archive.skipped.is_empty() {
                    content = content.child(text(format!(
                        "未处理的数据：{}",
                        archive.skipped.join("、")
                    )));
                }
                let Some(group) = archive.groups.get(index) else {
                    return content.child(text("此文件没有原神祈愿记录"));
                };
                content
                    .child(group_options(&archive, index, &controls))
                    .child(target_options(group, &archive, index, profile, &controls))
                    .child(controls.preview.get().map(|preview| {
                        let controls = controls.clone();
                        view()
                            .gap_2()
                            .child(text(preview.summary()))
                            .child(
                                preview
                                    .has_conflicts()
                                    .then(|| text("冲突记录将跳过，已保存记录会保留")),
                            )
                            .child((!preview.additions.is_empty()).then(|| {
                                button("备份并确认导入", move || {
                                    controls.import(&archive, index, &preview);
                                })
                            }))
                    }))
            })
        })
    }
}

fn group_options(
    archive: &Archive,
    selected: usize,
    controls: &ImportControls,
) -> impl IntoElement {
    view().flex_row().flex_wrap().gap_2().children(
        archive
            .groups
            .iter()
            .enumerate()
            .map(|(index, group)| {
                let label = group.uid.as_deref().map_or_else(
                    || format!("记录组 {}（UID 无法识别）", index + 1),
                    |uid| format!("记录组 {}（UID {uid}）", index + 1),
                );
                if index == selected {
                    text(format!("已选{label}")).into_element()
                } else {
                    let group = controls.group;
                    let preview = controls.preview;
                    button(label, move || {
                        preview.set(None);
                        group.set(index);
                    })
                    .into_element()
                }
            })
            .collect::<Vec<_>>(),
    )
}

fn target_options(
    group: &Group,
    archive: &Rc<Archive>,
    index: usize,
    profile: Signal<Profile>,
    controls: &ImportControls,
) -> impl IntoElement {
    let mut targets = profile.get().uids;
    let instruction = if let Some(uid) = &group.uid {
        targets.retain(|target| target == uid);
        targets
            .is_empty()
            .then(|| format!("请先创建 UID {uid} 的数据空间"))
    } else if targets.is_empty() {
        Some("文件 UID 无法识别，请先创建一个 UID 数据空间".to_owned())
    } else {
        Some("文件 UID 无法识别，请明确选择导入目标".to_owned())
    };
    view().gap_2().child(instruction.map(text)).child(
        view().flex_row().flex_wrap().gap_2().children(
            targets
                .into_iter()
                .map(|uid| {
                    let controls = controls.clone();
                    let archive = archive.clone();
                    button(format!("预览导入至 UID {uid}"), move || {
                        controls.preview(&uid, &archive.groups[index]);
                    })
                    .into_element()
                })
                .collect::<Vec<_>>(),
        ),
    )
}

/// Renders file selection, import preview and UID wish history.
pub(super) fn section(
    database: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    let heading = view()
        .gap_2()
        .child(text("祈愿记录").font_weight(FontWeight::BOLD));
    let database = match database {
        Ok(path) => Rc::new(path),
        Err(error) => return heading.child(text(error)),
    };
    let controls = ImportControls {
        database: database.clone(),
        input: signal(String::new()),
        archive: signal(None),
        group: signal(0),
        preview: signal(None),
        message: signal(String::new()),
        revision,
    };
    heading
        .child(button("UIGF 数据格式规范", || {
            drop(open_url("https://uigf.org/zh/standards/UIGF.html"));
        }))
        .child(controls.file_input())
        .child(controls.loaded(profile))
        .child(text(move || controls.message.get()))
        .child(history(database, profile, revision))
}

fn history(
    database: Rc<PathBuf>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> impl IntoElement {
    let position = signal((String::new(), 0_i64));
    let export_message = signal(String::new());
    let owner = Owner::current();
    view()
        .gap_2()
        .child(dynamic(move || {
            revision.get();
            let uid = profile.get().active_uid?;
            let (saved_uid, index) = position.get();
            let index = if uid == saved_uid { index } else { 0 };
            let mut content = view().gap_2().child(text(format!("UID {uid} 的祈愿记录")));
            match store::page(&database, &uid, index) {
                Ok((0, _)) => Some(content.child(text("暂无祈愿记录"))),
                Ok((total, records)) => {
                    let index = index.min((total - 1) / PAGE_SIZE);
                    let next_uid = uid.clone();
                    let previous_uid = uid.clone();
                    content = content
                        .child(text(format!(
                            "共 {total} 条 · 第 {} 页 · 时间 UTC",
                            index + 1
                        )))
                        .children(records.iter().map(|wish| text(wish.label()).into_element()))
                        .child(
                            view()
                                .flex_row()
                                .gap_2()
                                .child((index > 0).then(|| {
                                    button("上一页", move || {
                                        position.set((previous_uid.clone(), index - 1));
                                    })
                                }))
                                .child(((index + 1) * PAGE_SIZE < total).then(|| {
                                    button("下一页", move || {
                                        position.set((next_uid.clone(), index + 1));
                                    })
                                })),
                        );
                    let export_database = database.clone();
                    let export_uid = uid.clone();
                    Some(
                        content
                            .child(button("导出 UIGF v4.2", move || {
                                let folder = export_database.with_file_name("wish-exports");
                                export_to(&export_database, &export_uid, &folder, export_message);
                            }))
                            .child(export_picker(owner, database.clone(), uid, export_message)),
                    )
                }
                Err(error) => Some(content.child(text(error))),
            }
        }))
        .child(text(move || export_message.get()))
}

fn export_to(
    database: &std::path::Path,
    uid: &str,
    folder: &std::path::Path,
    message: Signal<String>,
) {
    match store::export(database, uid, folder) {
        Ok(path) => message.set(format!("UID {uid} 的 UIGF v4.2 已导出：{}", path.display())),
        Err(error) => message.set(format!("导出失败：{error}")),
    }
}

fn export_picker(
    owner: Owner,
    database: Rc<PathBuf>,
    uid: String,
    message: Signal<String>,
) -> impl IntoElement {
    button("导出到其他目录", move || {
        let database = database.clone();
        let uid = uid.clone();
        owner.run(move || {
            let request = pick_paths(OpenDialog::folders().title("选择 UIGF 导出目录"));
            spawn(async move {
                match request.await {
                    Ok(Some(paths)) => {
                        if let Some(folder) = paths.first() {
                            export_to(&database, &uid, folder, message);
                        }
                    }
                    Ok(None) => (),
                    Err(error) => message.set(error),
                }
            });
        });
    })
}
