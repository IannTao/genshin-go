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

#[derive(Clone, Default, PartialEq)]
struct InstallView {
    parent: Option<PathBuf>,
    pending: Option<PathBuf>,
    busy: bool,
    picking: bool,
    message: String,
    progress: Option<Progress>,
}

/// Returns the download controls and a shortcut to choose the installation location.
pub(crate) fn controls(
    environment: Rc<dyn LauncherEnvironment>,
    on_installed: impl Fn(PathBuf) + 'static,
) -> (impl IntoElement, impl Fn() + 'static) {
    let enabled = environment.supports_install();
    let owner = Owner::current();
    let mut initial = InstallView::default();
    if enabled {
        match environment.pending_install() {
            Ok(Some(path)) => {
                initial.pending = Some(path);
                initial.message = "有未完成的安装，等待手动继续".into();
            }
            Ok(None) => {}
            Err(error) => initial.message = format!("读取安装任务失败：{error}"),
        }
    }
    let state = signal(initial);
    let active = Rc::new(RefCell::new(None::<InstallControl>));
    let cleanup_active = active.clone();
    cleanup(move || {
        if let Some(control) = cleanup_active.borrow_mut().take() {
            control.pause();
        }
    });

    let pause_active = active.clone();
    let on_installed = Rc::new(on_installed);
    let session = Rc::new(InstallSession {
        owner,
        state,
        active,
        environment,
        on_installed,
    });
    let home_picker = session.clone();
    let controls = view().gap_2().child(dynamic(move || {
        if !enabled {
            return Vec::new();
        }
        let current = state.get();
        let has_pending = current.pending.is_some();
        let mut items = vec![text("下载国服官服").into_element()];
        if let Some(path) = current
            .pending
            .or_else(|| current.parent.as_ref().map(|p| p.join(DIRECTORY_NAME)))
        {
            items.push(text(format!("安装目录：{}", path.display())).into_element());
        }
        if !current.message.is_empty() {
            items.push(text(current.message).into_element());
        }
        if let Some(Progress { written, total }) = current.progress {
            let percent = (u128::from(written) * 100)
                .checked_div(u128::from(total))
                .unwrap_or(0);
            items.push(
                text(format!("已写入 {percent}%（{written} / {total} 字节）")).into_element(),
            );
        }
        if current.busy {
            let pause_active = pause_active.clone();
            items.push(
                button("暂停下载", move || {
                    if let Some(control) = pause_active.borrow().as_ref() {
                        control.pause();
                        state.update(|view| view.message = "正在暂停…".into());
                    }
                })
                .into_element(),
            );
        } else if has_pending {
            let session = session.clone();
            items.push(button("继续下载", move || session.start()).into_element());
        } else if !current.picking {
            let picker = session.clone();
            items.push(button("选择安装位置", move || picker.pick()).into_element());
            if current.parent.is_some() {
                let session = session.clone();
                items.push(button("开始下载", move || session.start()).into_element());
            }
        }
        items
    }));
    (controls, move || home_picker.pick())
}

struct InstallSession {
    owner: Owner,
    state: Signal<InstallView>,
    active: Rc<RefCell<Option<InstallControl>>>,
    environment: Rc<dyn LauncherEnvironment>,
    on_installed: Rc<dyn Fn(PathBuf)>,
}

impl InstallSession {
    fn start(self: &Rc<Self>) {
        let state = self.state;
        let current = state.get_untracked();
        if current.busy || current.picking {
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
            view.busy = true;
            view.message = "正在下载并安装…".into();
        });
        let operation = self.environment.install_game(parent, control.clone());
        let session = self.clone();
        self.owner.run(|| {
            let progress_control = control.clone();
            spawn(async move {
                while !progress_control.is_paused() {
                    let next = progress_control.progress();
                    if next.is_some() {
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
            view.busy = false;
            if let Ok(pending) = pending {
                view.pending = pending;
            }
            view.progress = control.progress().or(view.progress);
            view.message = match &result {
                Ok(Some(_)) => {
                    view.parent = None;
                    view.pending = None;
                    view.progress = None;
                    "游戏下载并安装完成".into()
                }
                Ok(None) => "下载已暂停".into(),
                Err(error) => format!("下载或安装失败：{error}"),
            };
        });
        if let Ok(Some(path)) = result {
            (self.on_installed)(path);
        }
    }

    fn pick(&self) {
        let state = self.state;
        if state.with_untracked(|view| view.busy || view.picking || view.pending.is_some()) {
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
                            view.message.clear();
                            view.progress = None;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            view.message = format!("无法选择安装位置：{error}");
                        }
                    }
                });
            })
        });
    }
}
