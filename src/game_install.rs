//! Connects the launcher UI to resumable installation operations.

use crate::launcher::{LauncherEnvironment, button};
use nsh_core::prelude::*;
use std::{
    cell::RefCell,
    future::Future,
    path::PathBuf,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[cfg(target_os = "windows")]
pub(crate) mod windows;

const DIRECTORY_NAME: &str = "Genshin Impact Game";

/// Resolves to the saved default installation, or to no path after pausing.
pub type InstallOperation = Pin<Box<dyn Future<Output = Result<Option<PathBuf>, String>>>>;

/// Shares pause requests and byte progress with an installation worker.
#[derive(Clone, Default)]
pub struct InstallControl(Arc<Control>);

#[derive(Default)]
struct Control {
    paused: AtomicBool,
    progress: Mutex<Option<Progress>>,
}

#[derive(Clone, Copy, PartialEq)]
struct Progress {
    written: u64,
    total: u64,
}

impl InstallControl {
    fn pause(&self) {
        self.0.paused.store(true, Ordering::Relaxed);
    }

    /// Indicates whether the worker should stop at a recoverable boundary.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.0.paused.load(Ordering::Relaxed)
    }

    /// Reports the verified bytes written and total bytes required.
    pub fn report(&self, written: u64, total: u64) {
        if let Ok(mut progress) = self.0.progress.lock() {
            *progress = Some(Progress { written, total });
        }
    }

    fn progress(&self) -> Option<Progress> {
        self.0.progress.lock().ok().and_then(|progress| *progress)
    }
}

impl Progress {
    fn percent(self) -> u128 {
        (u128::from(self.written) * 100)
            .checked_div(u128::from(self.total))
            .unwrap_or(0)
    }
}

#[derive(Clone, Default, PartialEq)]
struct InstallView {
    parent: Option<PathBuf>,
    pending: Option<PathBuf>,
    picking: bool,
    phase: InstallPhase,
    progress: Option<Progress>,
    notice: Option<String>,
}

#[derive(Clone, Default, PartialEq)]
enum InstallPhase {
    #[default]
    Setup,
    Downloading,
    Pausing,
    Paused,
    Interrupted(String),
    Restored,
    Complete,
}

impl InstallPhase {
    fn in_progress(&self) -> bool {
        matches!(self, Self::Downloading | Self::Pausing)
    }

    fn can_continue(&self) -> bool {
        matches!(self, Self::Paused | Self::Interrupted(_) | Self::Restored)
    }

    fn has_task(&self) -> bool {
        self.in_progress() || self.can_continue()
    }

    fn message(&self, show_reason: bool) -> &str {
        match self {
            Self::Setup => "",
            Self::Downloading => "正在下载并安装…",
            Self::Pausing => "正在暂停…",
            Self::Paused => "下载已暂停",
            Self::Interrupted(error) if show_reason => error,
            Self::Interrupted(_) => "下载中断，请前往游戏管理查看原因",
            Self::Restored => "有未完成的安装，等待手动继续",
            Self::Complete => "游戏下载并安装完成",
        }
    }
}

impl InstallView {
    fn sidebar_label(&self) -> String {
        let status = match self.phase {
            InstallPhase::Downloading => "下载中",
            InstallPhase::Pausing => "正在暂停…",
            InstallPhase::Paused => "下载已暂停",
            InstallPhase::Interrupted(_) => return "下载中断 · 查看原因".into(),
            InstallPhase::Restored => "下载待继续",
            _ => return String::new(),
        };
        self.progress.map_or_else(
            || status.into(),
            |progress| format!("{status} · {}%", progress.percent()),
        )
    }

    fn destination(&self) -> String {
        self.pending
            .clone()
            .or_else(|| {
                self.parent
                    .as_ref()
                    .map(|parent| parent.join(DIRECTORY_NAME))
            })
            .map_or_else(String::new, |path| format!("安装目录：{}", path.display()))
    }

    fn progress_label(&self) -> String {
        if self.phase == InstallPhase::Restored {
            return "继续下载后将校验已下载的进度".into();
        }
        self.progress.map_or_else(String::new, |progress| {
            format!(
                "已写入 {}%（{} / {} 字节）",
                progress.percent(),
                progress.written,
                progress.total
            )
        })
    }

    fn can_pick(&self) -> bool {
        !self.picking && !self.phase.in_progress() && self.pending.is_none()
    }

    fn can_continue(&self) -> bool {
        self.phase.can_continue() && !self.picking && self.has_destination()
    }

    fn has_destination(&self) -> bool {
        self.pending.is_some() || self.parent.is_some()
    }

    fn show_at_top(&self) -> bool {
        self.phase.has_task() || self.has_destination()
    }
}

/// Owns the installation task shared across the launcher pages.
pub(crate) struct InstallSession {
    owner: Owner,
    state: Signal<InstallView>,
    active: RefCell<Option<InstallControl>>,
    environment: Rc<dyn LauncherEnvironment>,
    on_installed: Box<dyn Fn(PathBuf)>,
}

impl InstallSession {
    /// Loads any saved task and pauses a running worker when the launcher closes.
    pub(crate) fn new(
        environment: Rc<dyn LauncherEnvironment>,
        on_installed: impl Fn(PathBuf) + 'static,
    ) -> Rc<Self> {
        let mut initial = InstallView::default();
        if environment.supports_install() {
            match environment.pending_install() {
                Ok(Some(path)) => {
                    initial.pending = Some(path);
                    initial.phase = InstallPhase::Restored;
                }
                Ok(None) => {}
                Err(error) => {
                    initial.notice = Some(format!("读取安装任务失败：{error}"));
                }
            }
        }
        let session = Rc::new(Self {
            owner: Owner::current(),
            state: signal(initial),
            active: RefCell::default(),
            environment,
            on_installed: Box::new(on_installed),
        });
        let cleanup_session = session.clone();
        cleanup(move || {
            if let Some(control) = cleanup_session.active.borrow_mut().take() {
                control.pause();
            }
        });
        session
    }

    /// Shows the download status link at the bottom of the sidebar.
    pub(crate) fn sidebar_status(&self, open_task: impl Fn() + 'static) -> View {
        let state = self.state;
        view()
            .mt(auto())
            .px_1()
            .py_2()
            .rounded(px(8.0))
            .text_size(px(12.0))
            .role(AccessibilityRole::Button)
            .style_when(
                move || !state.with(|view| view.phase.has_task()),
                Styled::hidden,
            )
            .on_press(open_task)
            .child(text(move || state.with(InstallView::sidebar_label)))
    }

    /// Shows download progress and available actions on the home page.
    pub(crate) fn home_status(self: &Rc<Self>) -> If {
        let state = self.state;
        let session = self.clone();
        when(
            move || {
                state.with(|view| view.phase.has_task() || view.phase == InstallPhase::Complete)
            },
            move || session.task_status(false),
        )
    }

    /// Indicates whether the home page can open the installation picker.
    pub(crate) fn can_offer_install(&self) -> bool {
        self.environment.supports_install()
            && self
                .state
                .with(|view| view.can_pick() && !view.phase.has_task())
    }

    /// Shows the selected installation destination or current task at the top of the page.
    pub(crate) fn details(self: &Rc<Self>) -> If {
        let state = self.state;
        let session = self.clone();
        when(
            move || state.with(InstallView::show_at_top),
            move || session.game_controls(),
        )
    }

    /// Offers installation setup below the game directory controls when there is no task or destination.
    pub(crate) fn setup(self: &Rc<Self>) -> If {
        let state = self.state;
        let enabled = self.environment.supports_install();
        let session = self.clone();
        when(
            move || enabled && !state.with(InstallView::show_at_top),
            move || session.game_controls(),
        )
    }

    fn game_controls(self: &Rc<Self>) -> View {
        let state = self.state;
        let picker = self.clone();
        let starter = self.clone();
        view()
            .gap_2()
            .items_start()
            .child(text("下载国服官服").font_weight(FontWeight::BOLD))
            .child(when(
                move || state.with(InstallView::has_destination),
                move || text(move || state.with(InstallView::destination)),
            ))
            .child(self.task_status(true))
            .child(when(
                move || state.with(|view| view.notice.is_some()),
                move || text(move || state.with(|view| view.notice.clone().unwrap_or_default())),
            ))
            .child(when(
                move || state.with(InstallView::can_pick),
                move || {
                    let picker = picker.clone();
                    button("选择安装位置", move || picker.pick())
                },
            ))
            .child(when(
                move || {
                    state.with(|view| {
                        view.parent.is_some() && !view.phase.has_task() && view.can_pick()
                    })
                },
                move || {
                    let starter = starter.clone();
                    button("开始下载", move || starter.start())
                },
            ))
    }

    fn task_status(self: &Rc<Self>, show_reason: bool) -> View {
        let state = self.state;
        let pauser = self.clone();
        let starter = self.clone();
        view()
            .gap_2()
            .items_start()
            .style_when(
                move || state.with(|view| view.phase == InstallPhase::Setup),
                Styled::hidden,
            )
            .child(text(move || {
                state.with(|view| view.phase.message(show_reason).to_owned())
            }))
            .child(when(
                move || {
                    state.with(|view| {
                        view.progress.is_some() || view.phase == InstallPhase::Restored
                    })
                },
                move || text(move || state.with(InstallView::progress_label)),
            ))
            .child(when(
                move || state.with(|view| view.phase == InstallPhase::Downloading),
                move || {
                    let pauser = pauser.clone();
                    button("暂停下载", move || pauser.pause())
                },
            ))
            .child(when(
                move || state.with(InstallView::can_continue),
                move || {
                    let starter = starter.clone();
                    button("继续下载", move || starter.start())
                },
            ))
    }

    fn pause(&self) {
        if let Some(control) = self.active.borrow().as_ref() {
            control.pause();
            self.state.update(|view| view.phase = InstallPhase::Pausing);
        }
    }

    fn start(self: &Rc<Self>) {
        let state = self.state;
        let current = state.get_untracked();
        if current.phase.in_progress() || current.picking {
            return;
        }
        let parent = if current.pending.is_some() {
            None
        } else if let Some(parent) = current.parent {
            Some(parent)
        } else {
            return;
        };
        let control = InstallControl::default();
        self.active.replace(Some(control.clone()));
        state.update(|view| {
            view.phase = InstallPhase::Downloading;
            view.notice = None;
        });
        let operation = self.environment.install_game(parent, control.clone());
        let session = self.clone();
        self.owner.run(|| {
            let progress_control = control.clone();
            spawn(async move {
                while !progress_control.is_paused() {
                    let next = progress_control.progress();
                    if next.is_some() && state.with_untracked(|view| view.progress != next) {
                        state.update(|view| view.progress = next);
                    }
                    timer(Duration::from_millis(200)).await;
                }
            });
            spawn(async move {
                let result = operation.await;
                control.pause();
                session.finish(&control, result);
            });
        });
    }

    fn finish(&self, control: &InstallControl, result: Result<Option<PathBuf>, String>) {
        let pending = self.environment.pending_install();
        self.active.replace(None);
        self.state.update(|view| {
            if let Ok(pending) = pending {
                view.pending = pending;
            }
            view.progress = control.progress().or(view.progress);
            view.phase = match &result {
                Ok(Some(_)) => {
                    view.parent = None;
                    view.pending = None;
                    view.progress = None;
                    InstallPhase::Complete
                }
                Ok(None) => InstallPhase::Paused,
                Err(error) => InstallPhase::Interrupted(format!("下载或安装失败：{error}")),
            };
        });
        if let Ok(Some(path)) = result {
            (self.on_installed)(path);
        }
    }

    /// Opens the installation picker when a new location can be chosen.
    pub(crate) fn pick(&self) {
        let state = self.state;
        if !state.with_untracked(InstallView::can_pick) {
            return;
        }
        state.update(|view| view.picking = true);
        let choice = self.environment.choose_install_parent();
        self.owner.run(|| {
            spawn(async move {
                let result = choice.await;
                state.update(|view| {
                    view.picking = false;
                    match result {
                        Ok(Some(path)) => {
                            view.parent = Some(path);
                            view.phase = InstallPhase::Setup;
                            view.progress = None;
                            view.notice = None;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            view.notice = Some(format!("无法选择安装位置：{error}"));
                        }
                    }
                });
            })
        });
    }
}
