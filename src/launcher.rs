//! Application boundary shared by the native root and the isolated test host.

mod home;
mod icons;
mod settings;
mod sidebar;
#[cfg(test)]
mod tests;
mod topbar;

use crate::game_install::{InstallControl, InstallOperation, InstallSession};
use home::{DirectoryControls, game_management, home};
use nsh_base::tooltip;
use nsh_core::element::{ImageSrc, svg};
use nsh_core::prelude::*;
use settings::{BackupSectionBuilder, settings_page};
use sidebar::{panel, sidebar};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    rc::Rc,
};
use topbar::topbar;

/// Represents an asynchronous directory selection result.
pub type DirectoryChoice = Pin<Box<dyn Future<Output = Result<Option<PathBuf>, String>>>>;

/// Reports whether the game process created by this launch is still running.
#[cfg(target_os = "windows")]
pub type GameProcess = Box<dyn Fn() -> bool>;

/// Owns the normal close request for a scanner started by the launcher.
#[cfg(target_os = "windows")]
pub struct ScannerInstance(Option<Box<dyn FnOnce()>>);

#[cfg(target_os = "windows")]
impl ScannerInstance {
    /// Requests closure of the dedicated instance when this value is dropped.
    pub fn new(close: impl FnOnce() + 'static) -> Self {
        Self(Some(Box::new(close)))
    }
}

#[cfg(target_os = "windows")]
impl Drop for ScannerInstance {
    fn drop(&mut self) {
        if let Some(close) = self.0.take() {
            close();
        }
    }
}

/// Returns the dedicated scanner instance opened for a single launch.
#[cfg(target_os = "windows")]
pub type ScannerLaunch = Pin<Box<dyn Future<Output = Result<ScannerInstance, String>>>>;

/// Provides native launcher operations backed by the specified application data directory.
#[must_use]
pub fn environment_in(directory: &Path) -> impl LauncherEnvironment + use<> {
    crate::native_environment::NativeEnvironment::in_directory(directory)
}

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
    /// Synchronizes growth material counts for the verified account via `MiYouShe`.
    #[cfg(target_os = "windows")]
    fn sync_mihoyo_materials(
        &self,
        _account: crate::profile::VerifiedAccount,
    ) -> crate::accounts::AccountOperation<std::collections::BTreeMap<String, u32>> {
        Box::pin(async { Err("本期仅支持 Windows 米游社材料同步".into()) })
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
    /// Requests a game launch and tracks its lifetime when Windows returns a process handle.
    #[cfg(target_os = "windows")]
    fn launch_with_process(&self, directory: &Path) -> Result<Option<GameProcess>, String> {
        self.launch(directory).map(|()| None)
    }
    /// Starts the selected capture scanner when its manager is unavailable.
    #[cfg(target_os = "windows")]
    fn start_scanner(&self, _program: &Path) -> ScannerLaunch {
        Box::pin(async { Err("无法启动 GGScanner".into()) })
    }
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
    /// Loads the curated plugin catalog for the Extensions page.
    fn load_plugin_catalog(&self) -> Result<crate::plugins::PluginCatalog, String> {
        crate::plugins::bundled_catalog()
    }
    /// Resolves the root directory where managed plugins are installed.
    fn plugins_root(&self) -> Option<PathBuf> {
        None
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

const ACCENT: AdaptiveColor = AdaptiveColor::rgb(0x0021_5b49, 0x002f_7d65);
const PAPER: AdaptiveColor = AdaptiveColor::rgb(0x00f4_f5ef, 0x0017_2420);
const SURFACE: AdaptiveColor = AdaptiveColor::rgb(0x00f5_f8f7, 0x0012_1d1a);
const CARD: AdaptiveColor = AdaptiveColor::rgb(0x00ff_fef8, 0x001d_2d28);
const BORDER: AdaptiveColor = AdaptiveColor::rgb(0x00dc_e4ea, 0x0029_4038);
const INK: AdaptiveColor = AdaptiveColor::rgb(0x0020_3832, 0x00f4_f5ef);
const MUTED_INK: AdaptiveColor = AdaptiveColor::rgb(0x0058_6e65, 0x009d_b5ac);
const CONTROL: AdaptiveColor = AdaptiveColor::rgb(0x00e0_e8da, 0x002a_473d);
const NAV_ACTIVE_BG: AdaptiveColor = AdaptiveColor::rgb(0x00e1_efea, 0x0021_4439);
const NAV_ACTIVE_INK: AdaptiveColor = AdaptiveColor::rgb(0x0013_6658, 0x008b_e0c5);
const NAV_HOVER_BG: AdaptiveColor = AdaptiveColor::rgb(0x00ea_f4ef, 0x001b_352d);
const TOOLTIP_BG: AdaptiveColor = AdaptiveColor::rgb(0x0015_2822, 0x00eb_f4f0);
const TOOLTIP_INK: AdaptiveColor = AdaptiveColor::rgb(0x00f4_f5ef, 0x0015_2822);

const SIDEBAR_EXPANDED_WIDTH: f32 = 212.0;
const SIDEBAR_COLLAPSED_WIDTH: f32 = 72.0;

#[derive(Clone, Copy)]
struct WorkbenchAppearance {
    sidebar_collapsed: Signal<bool>,
}

impl WorkbenchAppearance {
    fn new() -> Self {
        Self {
            sidebar_collapsed: signal(false),
        }
    }

    fn is_dark() -> bool {
        dark()
    }

    fn set_dark(dark_mode: bool) {
        set_color_scheme(if dark_mode {
            Appearance::Dark
        } else {
            Appearance::Light
        });
    }

    fn toggle_dark() {
        Self::set_dark(!untracked(dark));
    }

    fn is_collapsed(self) -> bool {
        self.sidebar_collapsed.get()
    }

    fn set_sidebar_collapsed(self, collapsed: bool) {
        self.sidebar_collapsed.set(collapsed);
    }

    fn toggle_sidebar(self) {
        self.set_sidebar_collapsed(!self.sidebar_collapsed.get_untracked());
    }

    fn sidebar_width(self) -> f32 {
        if self.is_collapsed() {
            SIDEBAR_COLLAPSED_WIDTH
        } else {
            SIDEBAR_EXPANDED_WIDTH
        }
    }

    fn content_width(self) -> f32 {
        (window().width.0 - self.sidebar_width() - 1.0).max(0.0)
    }

    fn narrow_content(self) -> bool {
        self.content_width() < 560.0
    }

    fn tight_content(self) -> bool {
        self.content_width() < 280.0
    }
}

fn icon_svg(markup: &'static str, size: f32) -> Svg {
    svg(ImageSrc::svg_markup(markup)).w(px(size)).h(px(size))
}

fn styled_tooltip<M>(
    placement: Placement,
    offset: f32,
    disabled: impl nsh_core::element::IntoProp<bool, M>,
    label: impl IntoReactive<SharedString>,
) -> nsh_base::TooltipElement {
    tooltip()
        .placement(placement)
        .offset(px(offset))
        .delay(150.0)
        .disabled(disabled)
        .px_2()
        .py_1()
        .rounded(px(6.0))
        .bg(TOOLTIP_BG)
        .text_color(TOOLTIP_INK)
        .child(text(label).text_size(px(12.0)))
}

fn button_frame(label: impl Into<String>) -> View {
    view()
        .px_3()
        .py_2()
        .rounded(px(8.0))
        .role(AccessibilityRole::Button)
        .child(text(label.into()).min_w(px(0.0)))
}

fn action_button(label: impl Into<String>, action: impl Fn() + 'static) -> View {
    action_button_with(label, action, None).0
}

fn action_button_with(
    label: impl Into<String>,
    action: impl Fn() + 'static,
    enabled: Option<Rc<dyn Fn() -> bool>>,
) -> (View, Option<crate::keyboard::FocusHandle>) {
    crate::keyboard::register_button(button_frame(label), Rc::new(action), enabled)
}

/// Renders an accessible button with the launcher's shared styling.
pub(crate) fn button(label: impl Into<String>, action: impl Fn() + 'static) -> impl IntoElement {
    action_button(label, action).bg(CONTROL).text_color(INK)
}

/// Renders a shared button whose handler receives its keyboard focus handle.
pub(crate) fn focusable_button(
    label: impl Into<String>,
    action: impl Fn(Option<crate::keyboard::FocusHandle>) + 'static,
) -> (View, Option<crate::keyboard::FocusHandle>) {
    let base = button_frame(label).bg(CONTROL).text_color(INK);
    crate::keyboard::register_button_with_handle(base, action, None)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Game,
    Community,
    Profile,
    #[cfg(target_os = "windows")]
    Wishes,
    #[cfg(target_os = "windows")]
    Inventory,
    #[cfg(target_os = "windows")]
    Achievements,
    Extensions,
    Settings,
}

#[derive(Clone, Copy)]
struct PageMeta {
    test_id: &'static str,
    icon: &'static str,
    label: &'static str,
}

impl Page {
    const MAIN: &'static [Self] = &[
        Self::Home,
        Self::Game,
        Self::Community,
        Self::Profile,
        #[cfg(target_os = "windows")]
        Self::Wishes,
        #[cfg(target_os = "windows")]
        Self::Inventory,
        #[cfg(target_os = "windows")]
        Self::Achievements,
        Self::Extensions,
    ];

    fn meta(self) -> PageMeta {
        match self {
            Self::Home => PageMeta {
                test_id: "nav-home",
                icon: icons::HOME,
                label: "首页",
            },
            Self::Game => PageMeta {
                test_id: "nav-game",
                icon: icons::GAMEPAD,
                label: "游戏管理",
            },
            Self::Community => PageMeta {
                test_id: "nav-community",
                icon: icons::CALENDAR,
                label: "米游社",
            },
            Self::Profile => PageMeta {
                test_id: "nav-profile",
                icon: icons::USERS,
                label: "角色资料",
            },
            #[cfg(target_os = "windows")]
            Self::Wishes => PageMeta {
                test_id: "nav-wishes",
                icon: icons::SPARKLES,
                label: "祈愿分析",
            },
            #[cfg(target_os = "windows")]
            Self::Inventory => PageMeta {
                test_id: "nav-inventory",
                icon: icons::PACKAGE,
                label: "背包仓库",
            },
            #[cfg(target_os = "windows")]
            Self::Achievements => PageMeta {
                test_id: "nav-achievements",
                icon: icons::TROPHY,
                label: "成就管理",
            },
            Self::Extensions => PageMeta {
                test_id: "nav-extensions",
                icon: icons::PUZZLE,
                label: "扩展",
            },
            Self::Settings => PageMeta {
                test_id: "nav-settings",
                icon: icons::SETTINGS,
                label: "系统设置",
            },
        }
    }

    fn label(self) -> &'static str {
        self.meta().label
    }
}

struct WorkbenchSession {
    page: Signal<Page>,
    appearance: WorkbenchAppearance,
    environment: Rc<dyn LauncherEnvironment>,
    profile: Signal<crate::profile::Profile>,
    profile_error: Option<String>,
    history_revision: Signal<u64>,
    growth_revision: Signal<u64>,
    #[cfg(target_os = "windows")]
    database: Option<Result<PathBuf, String>>,
    #[cfg(target_os = "windows")]
    scanner: Option<Rc<crate::scanner::Sync>>,
}

/// Renders the same launcher with either the Windows environment or a test host.
pub fn launcher(environment: Rc<dyn LauncherEnvironment>) -> impl IntoElement {
    let _keyboard = crate::keyboard::KeyboardFocus::install();
    let page = signal(Page::Home);
    let appearance = WorkbenchAppearance::new();
    let game_panel = element_ref();
    let open_install = move || {
        page.set(Page::Game);
        game_panel.scroll_to_offset(px(0.0), false);
    };
    let state = signal(find_installations(environment.as_ref()));
    let (profile, profile_error) = load_profile(environment.as_ref());
    let history_revision = signal(0_u64);
    let growth_revision = signal(0_u64);
    #[cfg(target_os = "windows")]
    let database = environment.uid_database_path();
    #[cfg(target_os = "windows")]
    let scanner = database.clone().map(|path| {
        crate::scanner::Sync::new(path, environment.clone(), profile, history_revision)
    });
    let directory = DirectoryControls::new(
        environment.clone(),
        state,
        #[cfg(target_os = "windows")]
        scanner.clone(),
    );
    let install = InstallSession::new(environment.clone(), move |path| set_default(state, path));
    let session = WorkbenchSession {
        page,
        appearance,
        environment,
        profile,
        profile_error,
        history_revision,
        growth_revision,
        #[cfg(target_os = "windows")]
        database,
        #[cfg(target_os = "windows")]
        scanner,
    };
    let content = workbench_content(session, game_panel, open_install, &directory, &install);
    view()
        .flex_1()
        .min_h(px(0.0))
        .bg(PAPER)
        .text_color(INK)
        .child(topbar(appearance))
        .child(view().h(px(1.0)).bg(BORDER))
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .flex_row()
                .child(sidebar(
                    page,
                    appearance,
                    install.sidebar_status(open_install),
                ))
                .child(view().w(px(1.0)).bg(BORDER))
                .child(content),
        )
}

fn workbench_content(
    session: WorkbenchSession,
    game_panel: ElementRef,
    open_install: impl Fn() + Copy + 'static,
    directory: &DirectoryControls,
    install: &Rc<InstallSession>,
) -> View {
    let WorkbenchSession {
        page,
        appearance,
        environment,
        profile,
        profile_error,
        history_revision,
        growth_revision,
        #[cfg(target_os = "windows")]
        database,
        #[cfg(target_os = "windows")]
        scanner,
    } = session;
    let community_env = environment.clone();
    let community_error = profile_error.clone();
    #[cfg(target_os = "windows")]
    let profile_database = database.clone();
    let profile_env = environment.clone();
    let extensions_env = environment.clone();
    #[cfg(target_os = "windows")]
    let (records, backup_section) = record_pages(
        page,
        appearance,
        environment,
        database,
        profile,
        history_revision,
        growth_revision,
    );
    #[cfg(not(target_os = "windows"))]
    let (records, backup_section): ([Scroll; 0], Option<BackupSectionBuilder>) = ([], None);
    let settings = panel(page, Page::Settings, appearance, move || {
        settings_page(appearance, backup_section)
    });
    view()
        .test_id("launcher-content")
        .flex_1()
        .min_w(px(0.0))
        // Hiding a page keeps its controls and pending work alive for this session.
        .child(panel(page, Page::Home, appearance, || {
            home(page, appearance, directory, install, open_install)
        }))
        .child(
            panel(page, Page::Game, appearance, || {
                #[cfg(target_os = "windows")]
                let scanner_settings = scanner.as_ref().map(crate::scanner::Sync::settings);
                #[cfg(not(target_os = "windows"))]
                let scanner_settings = None::<View>;
                game_management(directory, install, scanner_settings)
            })
            .element_ref(&game_panel),
        )
        .child(panel(page, Page::Community, appearance, || {
            view()
                .gap_4()
                .child(community_error.map(text))
                .child(crate::accounts::community_page(community_env, profile))
        }))
        .child(panel(page, Page::Profile, appearance, || {
            #[cfg(target_os = "windows")]
            let roster = scanner.as_ref().map(crate::scanner::Sync::roster);
            #[cfg(not(target_os = "windows"))]
            let roster = None::<View>;
            #[cfg(target_os = "windows")]
            let growth = profile_database.map(|path| {
                crate::growth::section(path, profile, growth_revision, history_revision)
            });
            #[cfg(not(target_os = "windows"))]
            let growth = None::<View>;
            crate::profile::spaces(
                profile_env,
                profile,
                history_revision,
                growth_revision,
                profile_error,
                roster,
                growth,
            )
        }))
        .child(panel(page, Page::Extensions, appearance, || {
            crate::plugins::section(extensions_env)
        }))
        .children(records.into_iter().map(IntoElement::into_element))
        .child(settings)
}

fn load_profile(
    environment: &dyn LauncherEnvironment,
) -> (Signal<crate::profile::Profile>, Option<String>) {
    match environment.load_profile() {
        Ok(profile) => (signal(profile), None),
        Err(error) => (
            signal(crate::profile::Profile::default()),
            Some(format!("读取本机数据失败：{error}")),
        ),
    }
}

#[cfg(target_os = "windows")]
fn record_pages(
    page: Signal<Page>,
    appearance: WorkbenchAppearance,
    environment: Rc<dyn LauncherEnvironment>,
    database: Option<Result<PathBuf, String>>,
    profile: Signal<crate::profile::Profile>,
    history_revision: Signal<u64>,
    growth_revision: Signal<u64>,
) -> ([Scroll; 3], Option<BackupSectionBuilder>) {
    let database = database.unwrap_or_else(|| Err("当前环境无法读取本机 UID 数据".into()));
    let inventory_database = database.clone();
    let inventory_opened = signal(false);
    effect(move || {
        if page.get() == Page::Inventory {
            inventory_opened.set(true);
        }
    });
    let wishes_database = database.clone();
    let achievements_database = database.clone();
    (
        [
            panel(page, Page::Wishes, appearance, move || {
                crate::wishes::section(wishes_database, profile, history_revision)
            }),
            panel(page, Page::Inventory, appearance, move || {
                when(
                    move || inventory_opened.get(),
                    move || {
                        crate::inventory::section(
                            environment.clone(),
                            inventory_database.clone(),
                            profile,
                            history_revision,
                        )
                    },
                )
            }),
            panel(page, Page::Achievements, appearance, move || {
                crate::achievements::section(achievements_database, profile, history_revision)
            }),
        ],
        Some(Box::new(move || {
            crate::backup::section(database, profile, history_revision, growth_revision)
        })),
    )
}
