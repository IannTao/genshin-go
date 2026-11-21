use super::{Service, Settings, Sync, store};
use crate::launcher::button;
use chrono::DateTime;
use nsh_base::theme;
use nsh_core::{
    element::{input, pick_paths},
    file_dialog::{FileFilter, OpenDialog},
    prelude::*,
};
use std::{path::PathBuf, rc::Rc};

fn port(value: &str) -> Result<u16, String> {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| "本机端口须为 1 至 65535".into())
}

fn field(id: &'static str, label: &'static str, value: Signal<String>) -> impl IntoElement {
    input()
        .test_id(id)
        .label(label)
        .placeholder(label)
        .h_10()
        .px_3()
        .bg(move || theme().secondary)
        .value(move || value.get())
        .on_change(move |s: &str| value.set(s.to_owned()))
}

impl Sync {
    /// Renders the local service configuration and connection check.
    pub(crate) fn settings(self: &Rc<Self>) -> View {
        let settings = self.path().and_then(Settings::load);
        let message = signal(settings.as_ref().err().cloned().unwrap_or_default());
        let saved = settings.ok().flatten();
        let program = signal(
            saved
                .as_ref()
                .map_or_else(String::new, |s| s.program.display().to_string()),
        );
        let port_text = signal(saved.map_or(8765, |s| s.port).to_string());
        let save = self.clone();
        let disable = self.clone();
        let owner = Owner::current();
        view().gap_2()
            .child(text("已拥有角色同步").font_weight(FontWeight::BOLD))
            .child(text("兼容 GGScanner v2026.11.15 Windows 抓包版。扫描器需要管理员权限，第三方采集工具可能带来游戏账号风险。"))
            .child(text("在 GGScanner 中选择原神，开启「持续同步（抓包）」并启动管理服务。保存设置后，普通启动会从本机端口读取本次登录的角色资料。"))
            .child(field("scanner-program", "GGScanner.exe 路径", program))
            .child(button("选择 GGScanner.exe", move || owner.run(|| {
                let choice = pick_paths(OpenDialog::files().title("选择 GGScanner.exe").filter(FileFilter::new("Windows 程序", ["exe"])));
                spawn(async move {
                    match choice.await {
                        Ok(Some(paths)) => if let Some(path) = paths.first() { program.set(path.display().to_string()); },
                        Ok(None) => (),
                        Err(error) => message.set(error),
                    }
                });
            })))
            .child(text("本机端口"))
            .child(field("scanner-port", "本机端口", port_text))
            .child(view().flex_row().flex_wrap().gap_2()
                .child(button("保存扫描设置", move || {
                    let result = port(&port_text.get_untracked()).and_then(|port| {
                        Settings { program: PathBuf::from(program.get_untracked().trim().trim_matches('"')), port }.save(save.path()?)
                    });
                    message.set(result.map_or_else(|e| e, |()| "已保存扫描设置".into()));
                }))
                .child(button("检查扫描连接", move || {
                    let service = port(&port_text.get_untracked()).and_then(Service::new);
                    message.set("正在检查扫描连接".into());
                    spawn(async move {
                        let result = match service {
                            Ok(service) => service.read().await.map(|r| if r.is_some() { "已连接原神持续同步服务" } else { "已连接持续同步服务，等待游戏登录" }.to_owned()),
                            Err(error) => Err(error),
                        };
                        message.set(result.unwrap_or_else(|error| format!("扫描连接检查失败：{error}")));
                    });
                }))
                .child(button("停用下次启动同步", move || {
                    let result = disable.path().and_then(Settings::disable);
                    message.set(result.map_or_else(|e| e, |()| "已停用下次启动同步".into()));
                })))
            .child(text(move || message.get()))
    }

    /// Shows the saved roster and both timestamps for the currently browsed UID.
    pub(crate) fn roster(self: &Rc<Self>) -> impl IntoElement {
        let sync = self.clone();
        dynamic(move || {
            sync.revision.get();
            let uid = sync.profile.with(|p| p.active_uid.clone())?;
            let mut section = view()
                .gap_2()
                .child(text("已拥有角色").font_weight(FontWeight::BOLD));
            match sync.path().and_then(|p| store::roster(p, &uid)) {
                Ok(Some(roster)) => {
                    section = section
                        .child(text(format!("角色记录：{} 条", roster.characters.len())))
                        .child(text(format!("来源时间：{}", time(roster.source))))
                        .child(text(format!("本机接收时间：{}", time(roster.received))));
                    for character in roster.characters {
                        let element = character
                            .element
                            .map_or_else(String::new, |e| format!("（{e}）"));
                        section = section.child(text(format!(
                            "{}{element} · 等级 {} · 突破 {} · 命座 {} · 天赋 {}/{}/{}",
                            character.key,
                            character.level,
                            character.ascension,
                            character.constellation,
                            character.talent.auto,
                            character.talent.skill,
                            character.talent.burst
                        )));
                    }
                    Some(section)
                }
                Ok(None) => Some(section.child(text("已拥有角色尚未同步"))),
                Err(error) => Some(section.child(text(error))),
            }
        })
    }
}

fn time(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms).map_or_else(
        || ms.to_string(),
        |t| t.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string(),
    )
}
