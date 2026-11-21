//! Coordinates Windows game launch and owned character synchronization.
mod protocol;
mod store;
mod ui;

use crate::{
    launcher::{GameProcess, LauncherEnvironment, ScannerInstance, button},
    profile::Profile,
};
use nsh_core::{element::timer, prelude::*};
use protocol::{ReadError, Service, Snapshot};
use std::{
    cell::RefCell,
    future::pending,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use store::Settings;
use tokio::sync::oneshot;

const PREPARATION_LIMIT: Duration = Duration::from_secs(15);

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
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
    phase: Signal<Phase>,
    interrupt: RefCell<Option<oneshot::Sender<()>>>,
    message: Signal<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Preparing,
    Synchronizing,
}

impl Sync {
    /// Creates the session state shared by the home, game and profile pages.
    pub(crate) fn new(
        database: Result<PathBuf, String>,
        environment: Rc<dyn LauncherEnvironment>,
        profile: Signal<Profile>,
        revision: Signal<u64>,
    ) -> Rc<Self> {
        Rc::new(Self {
            database,
            environment,
            profile,
            revision,
            phase: signal(Phase::Idle),
            interrupt: RefCell::new(None),
            message: signal(String::new()),
        })
    }

    fn path(&self) -> Result<&Path, String> {
        self.database.as_deref().map_err(Clone::clone)
    }

    /// Displays the scanner result independently of the game's launch result.
    pub(crate) fn status(self: &Rc<Self>) -> impl IntoElement {
        let message = self.message;
        let phase = self.phase;
        let skip = self.clone();
        let cancel = self.clone();
        view()
            .gap_2()
            .style_when(move || message.get().is_empty(), Styled::hidden)
            .child(text(move || message.get()))
            .child(
                view()
                    .style_when(move || phase.get() != Phase::Preparing, Styled::hidden)
                    .child(button("直接启动游戏", move || {
                        skip.interrupt(Phase::Preparing);
                    })),
            )
            .child(
                view()
                    .style_when(move || phase.get() != Phase::Synchronizing, Styled::hidden)
                    .child(button("取消本次同步", move || {
                        cancel.interrupt(Phase::Synchronizing);
                    })),
            )
    }

    fn interrupt(&self, phase: Phase) {
        if self.phase.get_untracked() == phase
            && let Some(sender) = self.interrupt.take()
        {
            let _ = sender.send(());
        }
    }

    fn begin(&self, phase: Phase) -> oneshot::Receiver<()> {
        let (sender, receiver) = oneshot::channel();
        self.interrupt.replace(Some(sender));
        self.phase.set(phase);
        receiver
    }

    /// Reports whether a launch or synchronization is already in progress.
    pub(crate) fn is_active(&self) -> bool {
        self.phase.get_untracked() != Phase::Idle
    }

    /// Starts the game after sampling the current session, then waits for the new login.
    pub(crate) fn launch(
        self: &Rc<Self>,
        start_game: impl FnOnce() -> Result<Option<GameProcess>, ()> + 'static,
    ) {
        if self.is_active() {
            return;
        }
        let settings = match self.path().and_then(Settings::load) {
            Ok(Some(settings)) => settings,
            result => {
                self.message.set(
                    result
                        .err()
                        .map_or_else(String::new, |error| format!("扫描失败：{error}")),
                );
                let _ = start_game();
                return;
            }
        };
        let skip = self.begin(Phase::Preparing);
        self.message.set("正在检查扫描连接".into());
        let sync = self.clone();
        spawn(async move {
            let message = sync.run(&settings, skip, start_game).await;
            sync.interrupt.take();
            sync.phase.set(Phase::Idle);
            sync.message.set(message);
        });
    }

    async fn run(
        &self,
        settings: &Settings,
        skip: oneshot::Receiver<()>,
        start_game: impl FnOnce() -> Result<Option<GameProcess>, ()>,
    ) -> String {
        let mut owned = None;
        let preparation = tokio::select! {
            biased;
            _ = skip => Err("已跳过本次同步".into()),
            prepared = tokio::time::timeout(PREPARATION_LIMIT, self.prepare(settings, &mut owned)) => {
                prepared
                    .unwrap_or_else(|_| Err("等待 GGScanner 原神管理服务就绪超时，请检查本机端口和持续同步设置".into()))
                    .map_err(|error| format!("扫描失败：{error}"))
            }
        };

        let (service, baseline) = match preparation {
            Ok(prepared) => prepared,
            Err(message) => {
                drop(owned);
                let _ = start_game();
                return message;
            }
        };
        let started = now_ms();
        let Ok(game) = start_game() else {
            return "游戏启动失败，已结束本次同步".into();
        };
        let started = match started {
            Ok(started) => started,
            Err(error) => return format!("扫描失败：{error}"),
        };
        let cancel = self.begin(Phase::Synchronizing);
        tokio::select! {
            biased;
            _ = cancel => "已取消本次同步".into(),
            () = game_exited(game) => "游戏已退出，已结束本次同步".into(),
            result = self.wait_for_login(&service, baseline.as_ref(), started) => {
                result.unwrap_or_else(|error| format!("扫描失败：{error}"))
            }
        }
    }

    async fn prepare(
        &self,
        settings: &Settings,
        owned: &mut Option<ScannerInstance>,
    ) -> Result<(Service, Option<Snapshot>), String> {
        let service = Service::new(settings.port)?;
        match service.read().await {
            Ok(baseline) => return Ok((service, baseline)),
            Err(ReadError::Failed(error)) => return Err(error),
            Err(ReadError::Unavailable) => {}
        }
        settings.validate_program()?;
        self.message.set("正在准备 GGScanner".into());
        *owned = Some(self.environment.start_scanner(&settings.program).await?);
        loop {
            match service.read().await {
                Ok(baseline) => return Ok((service, baseline)),
                Err(ReadError::Failed(error)) => return Err(error),
                Err(ReadError::Unavailable) => timer(Duration::from_millis(300)).await,
            }
        }
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
            let Some(mut snapshot) = service.read().await.map_err(ReadError::message)? else {
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

async fn game_exited(game: Option<GameProcess>) {
    let Some(running) = game else {
        return pending().await;
    };
    while running() {
        timer(Duration::from_secs(1)).await;
    }
}
