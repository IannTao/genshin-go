//! Reads owned characters from an existing Windows `GGScanner` manager.
mod protocol;
mod store;
mod ui;

use crate::profile::Profile;
use nsh_core::{element::timer, prelude::*};
use protocol::{Service, Snapshot};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use store::Settings;

/// Defines the local scanner settings and UID-owned roster tables.
pub(crate) const SCHEMA: &str = "
    CREATE TABLE scanner_settings (
        singleton INTEGER PRIMARY KEY CHECK(singleton=1),
        program TEXT NOT NULL, port INTEGER NOT NULL CHECK(port BETWEEN 1 AND 65535)
    );
    CREATE TABLE owned_characters (
        uid TEXT PRIMARY KEY NOT NULL REFERENCES uid_history(uid) ON DELETE CASCADE,
        session TEXT NOT NULL, revision INTEGER NOT NULL,
        source_ms INTEGER NOT NULL, received_ms INTEGER NOT NULL, characters TEXT NOT NULL
    );";

fn now_ms() -> Result<i64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .ok_or_else(|| "本机时间无效".into())
}

/// Coordinates one ordinary launch with the scanner and the currently browsed profile.
pub(crate) struct Sync {
    database: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
    active: Signal<bool>,
    message: Signal<String>,
}

impl Sync {
    /// Creates the session state shared by the home, game and profile pages.
    pub(crate) fn new(
        database: Result<PathBuf, String>,
        profile: Signal<Profile>,
        revision: Signal<u64>,
    ) -> Rc<Self> {
        Rc::new(Self {
            database,
            profile,
            revision,
            active: signal(false),
            message: signal(String::new()),
        })
    }

    fn path(&self) -> Result<&Path, String> {
        self.database.as_deref().map_err(Clone::clone)
    }

    /// Displays the scanner result independently of the game's launch result.
    pub(crate) fn status(&self) -> impl IntoElement {
        let message = self.message;
        text(move || message.get()).style_when(move || message.get().is_empty(), Styled::hidden)
    }

    /// Starts the game after sampling the current session, then waits for the new login.
    pub(crate) fn launch(self: &Rc<Self>, start_game: impl FnOnce() -> bool + 'static) {
        if self.active.get_untracked() {
            return;
        }
        let port = match self.path().and_then(Settings::load) {
            Ok(Some(settings)) => settings.port,
            result => {
                self.message.set(
                    result
                        .err()
                        .map_or_else(String::new, |error| format!("扫描失败：{error}")),
                );
                start_game();
                return;
            }
        };
        self.active.set(true);
        self.message.set("正在检查扫描连接".into());
        let sync = self.clone();
        spawn(async move {
            let service = Service::new(port);
            let baseline = match &service {
                Ok(service) => service.read().await,
                Err(error) => Err(error.clone()),
            };
            let started = now_ms();
            let launched = start_game();
            let result = match (service, baseline, started) {
                (Ok(service), Ok(baseline), Ok(started)) if launched => {
                    sync.wait_for_login(&service, baseline.as_ref(), started)
                        .await
                }
                (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => Err(error),
                _ => Ok("游戏启动失败，已结束本次同步".into()),
            };
            sync.message
                .set(result.unwrap_or_else(|error| format!("扫描失败：{error}")));
            sync.active.set(false);
        });
    }

    async fn wait_for_login(
        &self,
        service: &Service,
        baseline: Option<&Snapshot>,
        started: i64,
    ) -> Result<String, String> {
        self.message.set("等待本次登录的角色数据".into());
        let mut login: Option<(String, String)> = None;
        let mut last_revision = 0;
        for _ in 0..150 {
            timer(Duration::from_secs(2)).await;
            let Some(mut snapshot) = service.read().await? else {
                continue;
            };
            if baseline.is_some_and(|old| old.session_id == snapshot.session_id)
                || snapshot.updated_at < started
            {
                continue;
            }
            let identity = (snapshot.session_id.clone(), snapshot.uid().to_owned());
            if login.get_or_insert_with(|| identity.clone()) != &identity {
                return Err("登录会话已改变，请重新普通启动".into());
            }
            if snapshot.revision <= last_revision {
                continue;
            }
            last_revision = snapshot.revision;
            let received = now_ms()?;
            if snapshot.updated_at > received {
                return Err("GGScanner 的来源时间无效".into());
            }
            let Some(characters) = snapshot.characters()? else {
                self.message.set("等待本次登录的完整角色列表".into());
                continue;
            };
            if let Some(profile) =
                store::save_roster(self.path()?, &snapshot, &characters, received)?
            {
                self.profile.set(profile);
                self.revision.update(|revision| *revision += 1);
                return Ok(format!(
                    "已同步 UID {} 的 {} 条已拥有角色记录",
                    snapshot.uid(),
                    characters.len()
                ));
            }
            return Ok("已保留本机最新的角色记录".into());
        }
        Err("等待本次登录的角色数据超时".into())
    }
}
