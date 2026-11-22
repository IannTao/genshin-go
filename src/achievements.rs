//! Provides the Windows launcher's achievement progress and UIAF file exchange.
/// Defines the achievement records shared by standard exchange and overall backups.
pub(crate) mod archive;
mod progress;
/// Stores and merges UID-owned achievement progress.
pub(crate) mod store;

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
    let content = view().gap_4();
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
        .child(progress::section(database, profile, revision))
}
