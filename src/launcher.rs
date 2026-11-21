//! Application boundary shared by the native root and the isolated test host.

use crate::game_install::{InstallControl, InstallOperation, InstallSession};
use base64::{Engine, engine::general_purpose::STANDARD};
use nsh_core::element::input;
use nsh_core::prelude::*;
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    rc::Rc,
    sync::LazyLock,
};

/// Represents an asynchronous directory selection result.
pub type DirectoryChoice = Pin<Box<dyn Future<Output = Result<Option<PathBuf>, String>>>>;

/// External operations used by the launcher's Windows and test environments.
pub trait LauncherEnvironment {
    /// Samples the community service date and cumulative system suspend time.
    fn community_clock(&self) -> Option<crate::community_sign_in::Clock> {
        None
    }
    /// Checks the saved account's community sign-in and submits when needed and permitted.
    fn community_sign_in(
        &self,
        _account: crate::profile::VerifiedAccount,
        _day: u64,
        _allow_submission: bool,
    ) -> crate::community_sign_in::Operation {
        Box::pin(async {
            Err(crate::community_sign_in::Failure::Other(
                "本期仅支持 Windows 社区签到".into(),
            ))
        })
    }
    /// Checks a saved account authorization with the remote service.
    fn check_authorization(&self, account_id: String) -> crate::accounts::AccountOperation<bool>;
    /// Creates a new QR authorization session.
    fn create_qr(&self) -> crate::accounts::AccountOperation<crate::accounts::QrSession>;
    /// Retrieves the current state of a QR authorization session.
    fn poll_qr(
        &self,
        qr: crate::accounts::QrSession,
    ) -> crate::accounts::AccountOperation<crate::accounts::QrStatus>;
    /// Loads the local UID spaces and current verified account.
    fn load_profile(&self) -> Result<crate::profile::Profile, String>;
    /// Selects a UID space, creating it if requested.
    fn select_uid(&self, uid: &str, create: bool) -> Result<crate::profile::Profile, String>;
    /// Clears the local business records of one UID while preserving its data space.
    fn delete_uid_history(&self, uid: &str) -> Result<crate::profile::Profile, String>;
    /// Identifies the local database used for Windows UID history, when available.
    #[cfg(target_os = "windows")]
    fn uid_database_path(&self) -> Option<Result<PathBuf, String>> {
        None
    }
    /// Resolves the unique game UID associated with the scanned authorization.
    fn verify_uid(
        &self,
        authorization: crate::accounts::Authorization,
    ) -> crate::accounts::AccountOperation<String>;
    /// Saves verified credentials if the expected account still matches.
    fn save_authorization(
        &self,
        authorization: crate::accounts::Authorization,
        uid: &str,
        expected: Option<&crate::profile::VerifiedAccount>,
    ) -> Result<crate::profile::Profile, String>;
    /// Removes the matching saved authorization and preserves all UID data.
    fn remove_authorization(
        &self,
        account: &crate::profile::VerifiedAccount,
    ) -> Result<crate::profile::Profile, String>;
    /// Discovers supported game installations.
    fn discover(&self) -> Result<Vec<PathBuf>, String>;
    /// Loads the saved default game directory.
    fn load_default(&self) -> Result<Option<PathBuf>, String>;
    /// Saves a default game directory.
    fn save_default(&self, directory: &Path) -> Result<(), String>;
    /// Validates and normalizes a game installation directory.
    fn validate(&self, directory: &Path) -> Result<PathBuf, String>;
    /// Requests a native launch from a validated directory.
    fn launch(&self, directory: &Path) -> Result<(), String>;
    /// Opens the platform's game directory picker.
    fn choose_directory(&self) -> DirectoryChoice;
    /// Opens the picker for the parent directory of a new installation.
    fn choose_install_parent(&self) -> DirectoryChoice {
        self.choose_directory()
    }
    /// Indicates whether the environment can install the Windows game.
    fn supports_install(&self) -> bool {
        false
    }
    /// Reads the destination of an unfinished installation.
    fn pending_install(&self) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
    /// Starts an installation under the given parent or resumes the saved task.
    fn install_game(&self, _parent: Option<PathBuf>, _control: InstallControl) -> InstallOperation {
        Box::pin(async { Err("本期仅支持 Windows 下载游戏".into()) })
    }
}

#[derive(Clone, PartialEq)]
enum SavedDirectory {
    Valid(PathBuf),
    Invalid(PathBuf),
}

#[derive(Clone, PartialEq)]
struct InstallationState {
    directories: Vec<PathBuf>,
    candidate: Option<PathBuf>,
    saved: Option<SavedDirectory>,
    message: String,
    editing: bool,
}

impl InstallationState {
    fn launch_directory(&self) -> Option<&Path> {
        match &self.saved {
            Some(SavedDirectory::Valid(path)) => Some(path),
            _ => None,
        }
    }

    fn chosen_candidate(&self) -> Option<&Path> {
        if let [only] = self.directories.as_slice() {
            Some(only)
        } else {
            self.candidate.as_deref()
        }
    }
}

fn find_installations(environment: &dyn LauncherEnvironment) -> InstallationState {
    let mut state = InstallationState {
        directories: Vec::new(),
        candidate: None,
        saved: None,
        message: String::new(),
        editing: false,
    };
    match environment.discover() {
        Ok(paths) => state.directories = paths,
        Err(error) => state.message = format!("自动查找失败：{error}。请选择游戏目录。"),
    }
    match environment.load_default() {
        Ok(Some(directory)) => match environment.validate(&directory) {
            Ok(directory) => state.saved = Some(SavedDirectory::Valid(directory)),
            Err(error) => {
                state.saved = Some(SavedDirectory::Invalid(directory));
                state.message = format!("默认目录已失效：{error}。请重新选择游戏目录。");
            }
        },
        Ok(None) => {}
        Err(error) => state.message = format!("读取默认目录失败：{error}。请重新选择游戏目录。"),
    }
    if state.message.is_empty() {
        state.message = if state.launch_directory().is_some() {
            "默认目录已载入"
        } else if state.directories.is_empty() {
            if environment.supports_install() {
                "未找到游戏。可选择已有游戏目录，或下载国服官服。"
            } else {
                "未找到游戏，请先安装国服官服，再选择游戏目录。"
            }
        } else {
            "请选择游戏目录并设为默认目录。"
        }
        .into();
    }
    state
}

fn select(environment: &dyn LauncherEnvironment, state: Signal<InstallationState>, path: &Path) {
    let directory = match environment.validate(path) {
        Ok(directory) => directory,
        Err(error) => {
            state.update(|s| s.message = format!("无法选择目录：{error}"));
            return;
        }
    };
    match environment.save_default(&directory) {
        Ok(()) => set_default(state, directory),
        Err(error) => state.update(|s| s.message = format!("无法保存默认目录：{error}")),
    }
}

fn set_default(state: Signal<InstallationState>, directory: PathBuf) {
    state.update(|s| {
        s.saved = Some(SavedDirectory::Valid(directory));
        s.candidate = None;
        s.message = "默认目录已保存".into();
        s.editing = false;
    });
}

const ACCENT: AdaptiveColor = AdaptiveColor::rgb(0x0021_5b49, 0x0021_5b49);
const PAPER: AdaptiveColor = AdaptiveColor::rgb(0x00f4_f5ef, 0x0017_2420);
const INK: AdaptiveColor = AdaptiveColor::rgb(0x0020_3832, 0x00f4_f5ef);
const CONTROL: AdaptiveColor = AdaptiveColor::rgb(0x00e0_e8da, 0x002a_473d);
const SIDEBAR_BACKGROUND: u32 = 0x0015_3e35;
const SIDEBAR_HOVER_BACKGROUND: u32 = 0x002a_5849;

static HOME_LANDSCAPE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(include_bytes!("../assets/home-landscape.jpg"))
    )
});

fn action_button(label: impl Into<String>, action: impl Fn() + 'static) -> View {
    view()
        .px_3()
        .py_2()
        .rounded(px(8.0))
        .role(AccessibilityRole::Button)
        .on_press(action)
        .child(text(label.into()))
}

/// Renders an accessible button with the launcher's shared styling.
pub(crate) fn button(label: impl Into<String>, action: impl Fn() + 'static) -> impl IntoElement {
    action_button(label, action).bg(CONTROL).text_color(INK)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Game,
    Community,
    Profile,
    Achievements,
}

impl Page {
    const ALL: [(Self, &'static str); 5] = [
        (Self::Home, "首页"),
        (Self::Game, "游戏管理"),
        (Self::Community, "米游社"),
        (Self::Profile, "角色资料"),
        (Self::Achievements, "成就"),
    ];
}

/// Renders the same launcher with either the Windows environment or a test host.
pub fn launcher(environment: Rc<dyn LauncherEnvironment>) -> impl IntoElement {
    let page = signal(Page::Home);
    let game_panel = element_ref();
    let open_install = move || {
        page.set(Page::Game);
        game_panel.scroll_to_offset(px(0.0), false);
    };
    let state = signal(find_installations(environment.as_ref()));
    let directory = DirectoryControls {
        environment: environment.clone(),
        state,
        picking: signal(false),
    };
    let (profile, profile_error) = match environment.load_profile() {
        Ok(profile) => (profile, None),
        Err(error) => (
            crate::profile::Profile::default(),
            Some(format!("读取本机数据失败：{error}")),
        ),
    };
    let profile = signal(profile);
    let history_revision = signal(0_u64);
    let install = InstallSession::new(environment.clone(), move |path| set_default(state, path));
    let community = crate::accounts::community_page(environment.clone(), profile);
    #[cfg(target_os = "windows")]
    let achievements = environment.uid_database_path().map(|database| {
        panel(
            page,
            Page::Achievements,
            crate::achievements::section(database, profile, history_revision),
        )
    });
    #[cfg(not(target_os = "windows"))]
    let achievements = None::<Scroll>;
    let uid_spaces = crate::profile::spaces(environment, profile, history_revision);
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_row()
        .bg(PAPER)
        .text_color(INK)
        .child(sidebar(
            page,
            install.sidebar_status(open_install),
            achievements.is_some(),
        ))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                // Hiding a page keeps its controls and pending work alive for this session.
                .child(panel(
                    page,
                    Page::Home,
                    home(page, &directory, &install, open_install),
                ))
                .child(
                    panel(
                        page,
                        Page::Game,
                        view()
                            .gap_4()
                            .child(install.details())
                            .child(directory.management())
                            .child(install.setup()),
                    )
                    .element_ref(&game_panel),
                )
                .child(panel(
                    page,
                    Page::Community,
                    view()
                        .gap_4()
                        .child(profile_error.clone().map(text))
                        .child(community),
                ))
                .child(panel(
                    page,
                    Page::Profile,
                    view()
                        .gap_2()
                        .child(profile_error.map(text))
                        .child(uid_spaces),
                ))
                .child(achievements),
        )
}

fn sidebar(page: Signal<Page>, download_status: View, has_achievements: bool) -> View {
    view()
        .w(|| {
            px(if compact_window() {
                70.0
            } else if narrow_window() {
                100.0
            } else {
                164.0
            })
        })
        .flex_shrink(0.0)
        .p(|| px(if narrow_window() { 8.0 } else { 12.0 }))
        .gap_2()
        .bg(rgb(SIDEBAR_BACKGROUND))
        .text_color(rgb(0x00f4_f3de))
        .child(
            text("Genshin-Go")
                .text_size(|| px(if narrow_window() { 12.0 } else { 14.0 }))
                .mb_3()
                .font_weight(FontWeight::BOLD)
                .style_when(compact_window, Styled::hidden),
        )
        .children(
            Page::ALL
                .into_iter()
                .filter(|(target, _)| *target != Page::Achievements || has_achievements)
                .map(|(target, label)| {
                    action_button(label, move || page.set(target))
                        .px(|| px(if compact_window() { 3.0 } else { 8.0 }))
                        .text_size(|| px(if compact_window() { 12.0 } else { 14.0 }))
                        .accessibility_state(move || AccessibilityState {
                            selected: page.get() == target,
                            ..Default::default()
                        })
                        .bg(move || {
                            if page.get() == target {
                                rgb(0x0037_6e5b)
                            } else {
                                rgb(SIDEBAR_BACKGROUND)
                            }
                        })
                        .hover(|style| style.bg(rgb(SIDEBAR_HOVER_BACKGROUND)))
                        .into_element()
                }),
        )
        .child(download_status.hover(|style| style.bg(rgb(SIDEBAR_HOVER_BACKGROUND))))
}

fn narrow_window() -> bool {
    window().width.0 < 700.0
}

fn compact_window() -> bool {
    window().width.0 < 340.0
}

fn panel(page: Signal<Page>, target: Page, content: impl IntoElement) -> Scroll {
    scroll()
        .flex_1()
        .style_when(move || page.get() != target, Styled::hidden)
        .child(
            view()
                .p(|| px(if narrow_window() { 12.0 } else { 28.0 }))
                .child(content.into_element()),
        )
}

fn home(
    page: Signal<Page>,
    directory: &DirectoryControls,
    install: &Rc<InstallSession>,
    open_install: impl Fn() + Copy + 'static,
) -> View {
    let state = directory.state;
    view()
        .flex_row()
        .rounded(px(12.0))
        .overflow_hidden()
        .bg(AdaptiveColor::rgb(0x00e5_ecdb, 0x0025_3b32))
        .child(
            view()
                .w(move || px((window().width.0 * 0.32).clamp(236.0, 332.0)))
                .max_w(relative(1.0))
                .flex_shrink(0.0)
                .min_h(|| px(if narrow_window() { 260.0 } else { 356.0 }))
                .p(|| {
                    px(if compact_window() {
                        8.0
                    } else if narrow_window() {
                        16.0
                    } else {
                        32.0
                    })
                })
                .gap_3()
                .justify_center()
                .items_start()
                .child(text("Windows · 国服官服").text_size(px(13.0)))
                .child(
                    text("原神启动器")
                        .text_size(|| px(if narrow_window() { 25.0 } else { 38.0 }))
                        .font_weight(FontWeight::BOLD),
                )
                .child(text(move || state.with(|state| state.message.clone())))
                .child(directory.home_actions(install, open_install))
                .child(button("管理游戏", move || page.set(Page::Game)))
                .child(install.home_status()),
        )
        .child(
            image(HOME_LANDSCAPE.as_str())
                .flex_1()
                .min_w(px(0.0))
                .object_fit(ResizeMode::Cover),
        )
}

fn launch(environment: &dyn LauncherEnvironment, state: Signal<InstallationState>) {
    let Some(path) = state.with_untracked(|state| state.launch_directory().map(Path::to_path_buf))
    else {
        return;
    };
    let directory = match environment.validate(&path) {
        Ok(directory) => directory,
        Err(error) => {
            state.update(|state| {
                state.directories.retain(|directory| directory != &path);
                state.candidate = None;
                state.saved = Some(SavedDirectory::Invalid(path));
                state.message = format!("启动失败：目录已失效：{error}。请重新选择游戏目录。");
            });
            return;
        }
    };
    let message = match environment.launch(&directory) {
        Ok(()) => "已请求启动游戏".into(),
        Err(error) => format!("启动失败：{error}"),
    };
    state.update(|state| state.message = message);
}

#[derive(Clone)]
struct DirectoryControls {
    environment: Rc<dyn LauncherEnvironment>,
    state: Signal<InstallationState>,
    picking: Signal<bool>,
}

impl DirectoryControls {
    fn home_actions(
        &self,
        install: &Rc<InstallSession>,
        open_install: impl Fn() + Copy + 'static,
    ) -> View {
        let state = self.state;
        let environment = self.environment.clone();
        let ready = move || state.with(|s| s.launch_directory().is_some());
        let picker = install.clone();
        let supports_install = self.environment.supports_install();
        view()
            .w_full()
            .min_w(px(0.0))
            .items_start()
            .child(
                action_button("普通启动", move || launch(environment.as_ref(), state))
                    .bg(ACCENT)
                    .text_color(white())
                    .px_4()
                    .py_3()
                    .style_when(move || !ready(), Styled::hidden),
            )
            .child(self.candidates().style_when(ready, Styled::hidden))
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .style_when(
                        move || ready() || state.with(|s| !s.directories.is_empty()),
                        Styled::hidden,
                    )
                    .child(self.directory_picker("选择已有游戏"))
                    .child(supports_install.then(|| {
                        button("下载安装", move || {
                            open_install();
                            if picker.can_offer_install() {
                                picker.pick();
                            }
                        })
                    })),
            )
    }

    fn candidates(&self) -> View {
        let state = self.state;
        let environment = self.environment.clone();
        view()
            .w_full()
            .min_w(px(0.0))
            .gap_2()
            .items_start()
            .style_when(
                move || state.with(|s| s.directories.is_empty()),
                Styled::hidden,
            )
            .child(dynamic(move || {
                let directories = state.with(|s| s.directories.clone());
                if let [only] = directories.as_slice() {
                    return vec![text(only.display().to_string()).w_full().into_element()];
                }
                directories
                    .into_iter()
                    .map(|path| {
                        let selected_path = path.clone();
                        action_button(format!("选择：{}", path.display()), move || {
                            state.update(|s| s.candidate = Some(path.clone()));
                        })
                        .w_full()
                        .accessibility_state(move || AccessibilityState {
                            selected: state.with(|s| s.candidate.as_ref() == Some(&selected_path)),
                            ..Default::default()
                        })
                        .bg(CONTROL)
                        .text_color(INK)
                        .selected(|style| style.bg(ACCENT).text_color(white()))
                        .into_element()
                    })
                    .collect::<Vec<_>>()
            }))
            .child(
                action_button("确认保存目录", move || {
                    if let Some(path) =
                        state.with_untracked(|s| s.chosen_candidate().map(Path::to_path_buf))
                    {
                        select(environment.as_ref(), state, &path);
                    }
                })
                .disabled(move || state.with(|s| s.chosen_candidate().is_none()))
                .bg(ACCENT)
                .text_color(white())
                .disabled_style(|style| style.bg(CONTROL).text_color(INK)),
            )
    }

    fn management(&self) -> View {
        let state = self.state;
        let manual = signal(String::new());
        let refresh_environment = self.environment.clone();
        let path_environment = self.environment.clone();
        view()
            .gap_2()
            .child(
                text("游戏目录")
                    .text_size(px(24.0))
                    .font_weight(FontWeight::BOLD),
            )
            .child(text(move || {
                state.with(|s| match &s.saved {
                    Some(SavedDirectory::Valid(path)) => format!("默认目录：{}", path.display()),
                    Some(SavedDirectory::Invalid(path)) => {
                        format!("已失效的默认目录：{}", path.display())
                    }
                    None => "尚未选择默认目录".into(),
                })
            }))
            .child(text(move || state.with(|s| s.message.clone())))
            .child(button("更换目录", move || {
                state.update(|s| s.editing = !s.editing);
            }))
            .child(
                view()
                    .gap_2()
                    .style_when(move || state.with(|s| !s.editing), Styled::hidden)
                    .child(button("重新查找", move || {
                        let mut refreshed = find_installations(refresh_environment.as_ref());
                        refreshed.editing = true;
                        state.set(refreshed);
                    }))
                    .child(self.candidates())
                    .child(self.directory_picker("选择游戏目录"))
                    .child(
                        input()
                            .test_id("game-directory")
                            .label("游戏目录")
                            .h_10()
                            .px_3()
                            .bg(CONTROL)
                            .placeholder("游戏目录（含 YuanShen.exe）")
                            .value(move || manual.get())
                            .on_change(move |value: &str| manual.set(value.to_owned())),
                    )
                    .child(button("使用此目录", move || {
                        select(
                            path_environment.as_ref(),
                            state,
                            Path::new(manual.get().trim().trim_matches('"')),
                        );
                    })),
            )
    }

    fn directory_picker(&self, label: &'static str) -> impl IntoElement {
        let Self {
            environment,
            state,
            picking,
        } = self.clone();
        action_button(label, move || {
            if picking.get_untracked() {
                return;
            }
            picking.set(true);
            let environment = environment.clone();
            spawn(async move {
                match environment.choose_directory().await {
                    Ok(Some(path)) => select(environment.as_ref(), state, &path),
                    Ok(None) => {}
                    Err(error) => state.update(|s| {
                        s.message = format!("目录选择失败：{error}。可在游戏管理页手动输入路径。");
                    }),
                }
                picking.set(false);
            });
        })
        .bg(CONTROL)
        .text_color(INK)
        .disabled(move || picking.get())
    }
}
