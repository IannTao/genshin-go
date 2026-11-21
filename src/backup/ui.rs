use super::{RestorePreview, export, preview, restore};
use crate::archive_json::file_button;
use crate::launcher::button;
use crate::profile::Profile;
use nsh_base::theme;
use nsh_core::element::input;
use nsh_core::prelude::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone)]
struct Controls {
    database: Rc<PathBuf>,
    file: Signal<String>,
    pending: Signal<Option<Rc<RestorePreview>>>,
    message: Signal<String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
}

impl Controls {
    fn set_file(&self, value: String) {
        self.file.set(value);
        self.pending.set(None);
        self.message.set(String::new());
    }

    fn export(&self) {
        self.message.set(match export(&self.database) {
            Ok(path) => format!("已创建整体备份：{}", path.display()),
            Err(error) => format!("创建整体备份失败：{error}"),
        });
    }

    fn preview(&self) {
        self.pending.set(None);
        let file = self.file.get_untracked();
        let file = file.trim().trim_matches('"');
        if file.is_empty() {
            self.message.set("请先选择整体备份文件".into());
            return;
        }
        match preview(&self.database, Path::new(file)) {
            Ok(pending) => {
                self.pending.set(Some(Rc::new(pending)));
                self.message.set(String::new());
            }
            Err(error) => self.message.set(error),
        }
    }

    fn confirm(&self, confirmed: &Rc<RestorePreview>) {
        if !self
            .pending
            .get_untracked()
            .is_some_and(|pending| Rc::ptr_eq(&pending, confirmed))
        {
            return;
        }
        self.pending.set(None);
        match restore(&self.database, confirmed) {
            Ok((backup, profile)) => {
                self.profile.set(profile);
                self.revision.update(|revision| *revision += 1);
                self.message
                    .set(format!("恢复完成。恢复前备份：{}", backup.display()));
            }
            Err(error) => self.message.set(format!("恢复失败：{error}")),
        }
    }

    fn render(&self) -> View {
        let export = self.clone();
        let edit = self.clone();
        let browse = self.clone();
        let preview = self.clone();
        let confirmation = self.clone();
        let file = self.file;
        let message = self.message;
        view()
            .gap_2()
            .child(text(
                "整体备份包含所有 UID 数据空间的祈愿记录和成就进度。换电脑后，请重新扫码获取米游社授权。",
            ))
            .child(button("创建整体备份", move || export.export()))
            .child(
                input()
                    .test_id("restore-file")
                    .label("整体备份文件路径")
                    .placeholder("整体备份文件路径")
                    .h_10()
                    .px_3()
                    .bg(move || theme().secondary)
                    .value(move || file.get())
                    .on_change(move |value: &str| edit.set_file(value.to_owned())),
            )
            .child(view().flex_row().flex_wrap().gap_2().child(file_button(
                "整体备份",
                move |result| match result {
                    Ok(path) => browse.set_file(path.display().to_string()),
                    Err(error) => message.set(error),
                },
            )).child(button("预览恢复", move || preview.preview())))
            .child(dynamic(move || {
                confirmation.pending.get().map(|pending| {
                    let confirmation = confirmation.clone();
                    let summaries = pending.entries.iter().map(|entry| {
                        view()
                            .child(entry.create.then(|| text(format!("新建 UID 数据空间：{}", entry.uid))))
                            .children(entry.summaries().map(text))
                    }).collect::<Vec<_>>();
                    view()
                        .gap_2()
                        .child(summaries)
                        .child(text("祈愿冲突保留本机记录，成就冲突按状态、进度与完成时间合并。"))
                        .child(if pending.has_changes() {
                            button("备份并确认恢复", move || confirmation.confirm(&pending)).into_element()
                        } else {
                            text("没有需要恢复的变更").into_element()
                        })
                })
            }))
            .child(text(move || message.get()))
    }
}

/// Renders backup controls and their preview in the UID spaces page.
pub(super) fn section(
    path: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    let open = signal(false);
    let content = match path {
        Ok(database) => Controls {
            database: Rc::new(database),
            file: signal(String::new()),
            pending: signal(None),
            message: signal(String::new()),
            profile,
            revision,
        }
        .render(),
        Err(error) => view().child(text(error)),
    };
    view()
        .gap_2()
        .child(button("备份与恢复", move || {
            open.update(|value| *value = !*value);
        }))
        .child(
            view()
                .style_when(move || !open.get(), Styled::hidden)
                .child(content),
        )
}
