//! Provides the Windows launcher's UIAF import, export, and achievement progress.
mod archive;
mod store;

use crate::archive_json::file_button;
use crate::launcher::button;
use crate::profile::Profile;
use archive::Archive;
use nsh_base::theme;
use nsh_core::element::{input, open_url};
use nsh_core::prelude::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use store::Preview;

const PAGE_SIZE: usize = 30;

#[derive(Clone)]
struct ImportControls {
    database: Rc<PathBuf>,
    path: Signal<String>,
    archive: Signal<Option<Rc<Archive>>>,
    preview: Signal<Option<Rc<Preview>>>,
    message: Signal<String>,
    revision: Signal<u64>,
}

impl ImportControls {
    fn set_path(&self, path: String) {
        self.path.set(path);
        self.archive.set(None);
        self.preview.set(None);
        self.message.set(String::new());
    }

    fn load(&self) {
        self.archive.set(None);
        self.preview.set(None);
        let path = self.path.get_untracked();
        let path = path.trim().trim_matches('"');
        if path.is_empty() {
            self.message.set("请先选择 UIAF JSON 文件".into());
            return;
        }
        match Archive::read(Path::new(path)) {
            Ok(archive) => {
                self.archive.set(Some(Rc::new(archive)));
                self.message.set(String::new());
            }
            Err(error) => self.message.set(error),
        }
    }

    fn preview(&self, uid: &str, archive: &Archive) {
        match store::preview(&self.database, uid, archive) {
            Ok(preview) => {
                self.preview.set(Some(Rc::new(preview)));
                self.message.set(String::new());
            }
            Err(error) => {
                self.preview.set(None);
                self.message.set(error);
            }
        }
    }

    fn import(&self, preview: &Rc<Preview>) {
        if !self
            .preview
            .get_untracked()
            .is_some_and(|current| Rc::ptr_eq(&current, preview))
        {
            return;
        }
        self.preview.set(None);
        match store::import(&self.database, preview) {
            Ok(backup) => {
                self.revision.update(|revision| *revision += 1);
                self.message.set(format!(
                    "已导入 UID {} 的成就进度。导入前备份：{}",
                    preview.uid,
                    backup.display()
                ));
            }
            Err(error) => self.message.set(format!("导入失败：{error}")),
        }
    }

    fn file_input(&self) -> View {
        let path = self.path;
        let edit = self.clone();
        let browse = self.clone();
        let load = self.clone();
        view()
            .gap_2()
            .child(
                input()
                    .test_id("uiaf-file")
                    .label("UIAF JSON 文件路径")
                    .placeholder("UIAF JSON 文件路径")
                    .h_10()
                    .px_3()
                    .bg(move || theme().secondary)
                    .value(move || path.get())
                    .on_change(move |value: &str| edit.set_path(value.to_owned())),
            )
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .child(file_button("UIAF", move |result| match result {
                        Ok(path) => {
                            browse.set_path(path.to_string_lossy().into_owned());
                            browse.load();
                        }
                        Err(error) => browse.message.set(error),
                    }))
                    .child(button("读取 UIAF 文件", move || load.load())),
            )
    }

    fn loaded(&self, profile: Signal<Profile>) -> impl IntoElement {
        let controls = self.clone();
        dynamic(move || {
            let archive = controls.archive.get()?;
            let uids = profile.get().uids;
            let mut content = view()
                .gap_2()
                .child(text(format!("已读取 {} 项成就", archive.records.len())));
            let targets = match &archive.uid {
                Some(uid) if uids.contains(uid) => vec![uid.clone()],
                Some(uid) => {
                    return Some(content.child(text(format!(
                        "文件声明的 UID 为 {uid}，请先在角色资料中创建该数据空间"
                    ))));
                }
                None if uids.is_empty() => {
                    return Some(content.child(text("请先在角色资料中创建 UID 数据空间")));
                }
                None => {
                    content = content.child(text("文件未声明 UID，请选择导入目标"));
                    uids
                }
            };
            Some(
                content
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .gap_2()
                            .children(targets.into_iter().map(|uid| {
                                let controls = controls.clone();
                                let archive = archive.clone();
                                button(format!("预览导入至 UID {uid}"), move || {
                                    controls.preview(&uid, &archive);
                                })
                                .into_element()
                            })),
                    )
                    .child(controls.preview.get().map(|preview| {
                        let controls = controls.clone();
                        view()
                            .gap_2()
                            .child(text(preview.summary()))
                            .child(text(preview.overlap()))
                            .child(preview.has_changes().then(|| {
                                button("备份并确认导入", move || controls.import(&preview))
                            }))
                    })),
            )
        })
    }
}

/// Renders the achievement tab for the launcher's UID data spaces.
pub(crate) fn section(
    database: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    let content = view().gap_4().child(
        text("成就进度")
            .text_size(px(24.0))
            .font_weight(FontWeight::BOLD),
    );
    let database = match database {
        Ok(path) => Rc::new(path),
        Err(error) => return content.child(text(error)),
    };
    let controls = ImportControls {
        database: database.clone(),
        path: signal(String::new()),
        archive: signal(None),
        preview: signal(None),
        message: signal(String::new()),
        revision,
    };
    content
        .child(button("UIAF v1.1 格式规范", || {
            drop(open_url("https://uigf.org/zh/standards/uiaf.html"));
        }))
        .child(controls.file_input())
        .child(controls.loaded(profile))
        .child(text(move || controls.message.get()))
        .child(progress(database, profile, revision))
}

fn progress(database: Rc<PathBuf>, profile: Signal<Profile>, revision: Signal<u64>) -> View {
    let position = signal((String::new(), 0_usize));
    let export_message = signal(String::new());
    view()
        .gap_2()
        .child(dynamic(move || {
            revision.get();
            let Some(uid) = profile.get().active_uid else {
                return view().child(text("请在角色资料中创建或选择游戏 UID"));
            };
            let content = view()
                .gap_2()
                .child(text(format!("UID {uid} 的成就进度")).font_weight(FontWeight::BOLD));
            let records = match store::records(&database, &uid) {
                Ok(records) if records.is_empty() => return content.child(text("暂无成就进度")),
                Ok(records) => records,
                Err(error) => return content.child(text(error)),
            };
            let (shown_uid, index) = position.get();
            let page_count = records.len().div_ceil(PAGE_SIZE);
            let index = if shown_uid == uid {
                index.min(page_count - 1)
            } else {
                0
            };
            let completed = records.iter().filter(|record| record.completed()).count();
            let previous = uid.clone();
            let next = uid.clone();
            let database = database.clone();
            content
                .child(text(format!(
                    "已完成 {completed} 项 · 共 {} 项 · 第 {}/{page_count} 页",
                    records.len(),
                    index + 1
                )))
                .children(
                    records
                        .iter()
                        .skip(index * PAGE_SIZE)
                        .take(PAGE_SIZE)
                        .map(|record| text(record.label()).into_element()),
                )
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .child((index > 0).then(|| {
                            button("上一页", move || {
                                position.set((previous.clone(), index - 1));
                            })
                        }))
                        .child((index + 1 < page_count).then(|| {
                            button("下一页", move || position.set((next.clone(), index + 1)))
                        })),
                )
                .child(button("导出 UIAF v1.1", move || {
                    let folder = database.with_file_name("achievement-exports");
                    match store::export(&database, &uid, &folder) {
                        Ok(path) => export_message
                            .set(format!("UID {uid} 的 UIAF v1.1 已导出：{}", path.display())),
                        Err(error) => export_message.set(format!("导出失败：{error}")),
                    }
                }))
        }))
        .child(text(move || export_message.get()))
}
