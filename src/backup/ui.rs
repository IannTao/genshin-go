use super::{Choices, RestorePreview, export, preview, restore};
use crate::archive_json::file_button;
use crate::growth::backup::{Target, Version};
use crate::launcher::button;
use crate::profile::Profile;
use nsh_base::theme;
use nsh_core::element::{SignalEquals, input, signal_with};
use nsh_core::prelude::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone)]
struct Controls {
    database: Rc<PathBuf>,
    file: Signal<String>,
    pending: Signal<Option<Rc<RestorePreview>>>,
    choices: Signal<Choices>,
    message: Signal<String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
    growth_revision: Signal<u64>,
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
                self.choices.set(Choices::new());
                self.pending.set(Some(Rc::new(pending)));
                self.message.set(String::new());
            }
            Err(error) => self.message.set(error),
        }
    }

    fn is_current(&self, preview: &Rc<RestorePreview>) -> bool {
        self.pending
            .get_untracked()
            .is_some_and(|pending| Rc::ptr_eq(&pending, preview))
    }

    fn confirm(&self, confirmed: &Rc<RestorePreview>) {
        if !self.is_current(confirmed) {
            return;
        }
        let choices = self.choices.get_untracked();
        if !confirmed.choices_complete(&choices) {
            return;
        }
        self.pending.set(None);
        match restore(&self.database, confirmed, &choices) {
            Ok((backup, profile)) => {
                if profile
                    .active_uid
                    .as_deref()
                    .is_some_and(|uid| confirmed.changes_growth_for(uid, &choices))
                {
                    self.growth_revision.update(|revision| *revision += 1);
                }
                self.profile.set(profile);
                self.revision.update(|revision| *revision += 1);
                self.message
                    .set(format!("恢复完成。恢复前备份：{}", backup.display()));
            }
            Err(error) => self.message.set(format!("恢复失败：{error}")),
        }
    }

    fn review(&self, pending: &Rc<RestorePreview>) -> View {
        let mut panel = view().gap_2();
        for entry in &pending.entries {
            panel = panel.child(
                view()
                    .child(
                        entry
                            .create
                            .then(|| text(format!("新建 UID 数据空间：{}", entry.uid))),
                    )
                    .children(entry.summaries().map(text)),
            );
            for (target, subject, local, backup) in entry.growth.conflicts() {
                let subject = format!("UID {} · {subject}", entry.uid);
                panel = panel.child(
                    view()
                        .gap_2()
                        .child(text(format!("冲突 · {subject}")))
                        .child(text(format!("本机版本 · {local}")))
                        .child(text(format!("备份版本 · {backup}")))
                        .child(self.conflict_options(pending, &subject, &entry.uid, target)),
                );
            }
        }
        let controls = self.clone();
        let pending = pending.clone();
        panel
            .child(text(
                "祈愿冲突保留本机记录，成就冲突按状态、进度与完成时间合并。",
            ))
            .child(dynamic(move || {
                let (complete, changes) = controls.choices.with(|choices| {
                    (
                        pending.choices_complete(choices),
                        pending.has_changes(choices),
                    )
                });
                if !complete {
                    return text("请为每项养成计划或手填库存冲突选择要保留的版本").into_element();
                }
                if !changes {
                    return text("没有需要恢复的变更").into_element();
                }
                let controls = controls.clone();
                let pending = pending.clone();
                button("备份并确认恢复", move || controls.confirm(&pending)).into_element()
            }))
    }

    fn conflict_options(
        &self,
        pending: &Rc<RestorePreview>,
        subject: &str,
        uid: &str,
        target: Target,
    ) -> View {
        let choices = self.choices;
        let uid = uid.to_owned();
        view()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .children(
                [(Version::Local, "保留本机"), (Version::Backup, "使用备份")].map(
                    |(version, label)| {
                        let controls = self.clone();
                        let pending = pending.clone();
                        let uid = uid.clone();
                        let target = target.clone();
                        button(format!("{label} · {subject}"), move || {
                            if controls.is_current(&pending) {
                                choices.update(|choices| {
                                    choices
                                        .entry(uid.clone())
                                        .or_default()
                                        .insert(target.clone(), version);
                                });
                            }
                        })
                    },
                ),
            )
            .child(text(move || {
                match choices.with(|choices| {
                    choices
                        .get(&uid)
                        .and_then(|selected| selected.get(&target))
                        .copied()
                }) {
                    Some(Version::Local) => "已选本机版本",
                    Some(Version::Backup) => "已选备份版本",
                    None => "请选择版本",
                }
            }))
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
                "整体备份包含所有 UID 数据空间的祈愿记录、成就进度、养成计划、手填库存和背包快照。换电脑后，请重新扫码获取米游社授权。",
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
                confirmation.pending.get().map(|pending| confirmation.review(&pending))
            }))
            .child(text(move || message.get()))
    }
}

/// Renders backup controls and their preview in the UID spaces page.
pub(super) fn section(
    path: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
    growth_revision: Signal<u64>,
) -> View {
    let open = signal(false);
    let content = match path {
        Ok(database) => Controls {
            database: Rc::new(database),
            file: signal(String::new()),
            pending: signal_with(None, SignalEquals::Never),
            choices: signal(Choices::new()),
            message: signal(String::new()),
            profile,
            revision,
            growth_revision,
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
