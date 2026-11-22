//! Application boundary shared by the native root and the isolated test host.

mod icons;
#[cfg(test)]
mod tests;

use crate::game_install::{InstallControl, InstallOperation, InstallSession};
use base64::{Engine, engine::general_purpose::STANDARD};
use nsh_base::tooltip;
use nsh_core::element::{ImageSrc, input, svg};
use nsh_core::prelude::*;
use nsh_core::{AccessibilityCurrent, Checked};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    rc::Rc,
    sync::LazyLock,
};

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
        .child(text(label.into()).min_w(px(0.0)))
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
    let directory = DirectoryControls {
        environment: environment.clone(),
        state,
        picking: signal(false),
        #[cfg(target_os = "windows")]
        scanner: scanner.clone(),
    };
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

type BackupSectionBuilder = Box<dyn FnOnce() -> View>;

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

fn topbar_icon_button(
    test_id: &'static str,
    label: impl Fn() -> &'static str + Copy + 'static,
    state: impl Fn() -> AccessibilityState + 'static,
    icon: impl Fn() -> &'static str + 'static,
    action: impl Fn() + 'static,
) -> View {
    view()
        .test_id(test_id)
        .w(px(34.0))
        .h(px(34.0))
        .rounded(px(6.0))
        .items_center()
        .justify_center()
        .role(AccessibilityRole::Button)
        .label(label)
        .accessibility_state(state)
        .on_press(action)
        .hover(|style| style.bg(NAV_HOVER_BG))
        .tooltip(styled_tooltip(Placement::Bottom, 6.0, false, label))
        .child(
            svg(move || ImageSrc::svg_markup(icon()))
                .w(px(20.0))
                .h(px(20.0)),
        )
}

fn topbar(appearance: WorkbenchAppearance) -> View {
    let sidebar_toggle = topbar_icon_button(
        "launcher-sidebar-toggle",
        move || {
            if appearance.is_collapsed() {
                "展开侧栏"
            } else {
                "收起侧栏"
            }
        },
        move || AccessibilityState {
            expanded: Some(!appearance.is_collapsed()),
            ..Default::default()
        },
        move || {
            if appearance.is_collapsed() {
                icons::PANEL_LEFT_OPEN
            } else {
                icons::PANEL_LEFT_CLOSE
            }
        },
        move || appearance.toggle_sidebar(),
    );
    let theme_toggle = topbar_icon_button(
        "launcher-theme-toggle",
        move || {
            if WorkbenchAppearance::is_dark() {
                "切换浅色模式"
            } else {
                "切换深色模式"
            }
        },
        move || AccessibilityState {
            pressed: Some(if WorkbenchAppearance::is_dark() {
                Checked::True
            } else {
                Checked::False
            }),
            ..Default::default()
        },
        move || {
            if WorkbenchAppearance::is_dark() {
                icons::SUN
            } else {
                icons::MOON
            }
        },
        WorkbenchAppearance::toggle_dark,
    );
    view()
        .test_id("launcher-topbar")
        .h(px(46.0))
        .flex_shrink(0.0)
        .flex_row()
        .items_center()
        .px_3()
        .gap_3()
        .bg(SURFACE)
        .text_color(INK)
        .child(sidebar_toggle)
        .child(
            view()
                .test_id("launcher-brand")
                .flex_row()
                .items_center()
                .gap_2()
                .child(icon_svg(icons::COMPASS, 20.0).text_color(ACCENT))
                .child(
                    text("Genshin Go")
                        .text_size(px(15.0))
                        .font_weight(FontWeight::BOLD),
                ),
        )
        .child(view().flex_1())
        .child(theme_toggle)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Appearance,
    #[cfg(target_os = "windows")]
    Backup,
}

fn settings_option_button(
    test_id: &'static str,
    label: &'static str,
    selected: impl Fn() -> bool + Copy + 'static,
    action: impl Fn() + 'static,
) -> View {
    action_button(label, action)
        .test_id(test_id)
        .accessibility_state(move || AccessibilityState {
            selected: selected(),
            ..Default::default()
        })
        .bg(move || if selected() { ACCENT } else { CONTROL })
        .text_color(move || if selected() { white().into() } else { INK })
        .style_when(selected, |style| style.font_weight(FontWeight::BOLD))
}

fn settings_card(title: &'static str, description: &'static str, controls: View) -> View {
    view()
        .p_4()
        .rounded(px(10.0))
        .bg(CARD)
        .gap_2()
        .child(
            text(title)
                .text_size(px(16.0))
                .font_weight(FontWeight::BOLD),
        )
        .child(text(description).text_size(px(13.0)).text_color(MUTED_INK))
        .child(controls)
}

fn appearance_settings(appearance: WorkbenchAppearance) -> View {
    view()
        .test_id("settings-appearance")
        .gap_3()
        .child(settings_card(
            "界面主题",
            "选择浅色或深色。顶栏与此处反映同一主题状态。",
            view()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(settings_option_button(
                    "theme-option-light",
                    "浅色",
                    move || !WorkbenchAppearance::is_dark(),
                    move || WorkbenchAppearance::set_dark(false),
                ))
                .child(settings_option_button(
                    "theme-option-dark",
                    "深色",
                    WorkbenchAppearance::is_dark,
                    move || WorkbenchAppearance::set_dark(true),
                )),
        ))
        .child(settings_card(
            "左侧导航",
            "收起后仅显示图标，悬停可查看模块名称。",
            view()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(settings_option_button(
                    "sidebar-option-expanded",
                    "展开",
                    move || !appearance.is_collapsed(),
                    move || appearance.set_sidebar_collapsed(false),
                ))
                .child(settings_option_button(
                    "sidebar-option-collapsed",
                    "仅图标",
                    move || appearance.is_collapsed(),
                    move || appearance.set_sidebar_collapsed(true),
                )),
        ))
}

fn settings_page(
    appearance: WorkbenchAppearance,
    backup_section: Option<BackupSectionBuilder>,
) -> View {
    let tab = signal(SettingsTab::Appearance);
    let mut tabs = view()
        .test_id("settings-tabs")
        .flex_row()
        .flex_wrap()
        .gap_2()
        .child(settings_option_button(
            "settings-tab-appearance",
            "外观",
            move || tab.get() == SettingsTab::Appearance,
            move || tab.set(SettingsTab::Appearance),
        ));
    #[cfg(target_os = "windows")]
    {
        tabs = tabs.child(settings_option_button(
            "settings-tab-backup",
            "备份与恢复",
            move || tab.get() == SettingsTab::Backup,
            move || tab.set(SettingsTab::Backup),
        ));
    }
    let appearance_panel = appearance_settings(appearance)
        .style_when(move || tab.get() != SettingsTab::Appearance, Styled::hidden);
    let mut root = view().gap_4().child(tabs).child(appearance_panel);
    #[cfg(target_os = "windows")]
    if let Some(backup) = backup_section {
        let backup_panel = view()
            .test_id("settings-backup")
            .style_when(move || tab.get() != SettingsTab::Backup, Styled::hidden)
            .child(backup());
        root = root.child(backup_panel);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = backup_section;
    root
}

fn game_management(
    directory: &DirectoryControls,
    install: &Rc<InstallSession>,
    scanner_settings: Option<View>,
) -> View {
    view()
        .gap_4()
        .child(install.details())
        .child(directory.management())
        .child(scanner_settings)
        .child(install.setup())
}

fn sidebar(page: Signal<Page>, appearance: WorkbenchAppearance, download_status: View) -> View {
    let settings = view()
        .test_id("launcher-sidebar-bottom")
        .gap_2()
        .child(view().h(px(1.0)).bg(BORDER))
        .child(page_button(page, Page::Settings, appearance));
    view()
        .test_id("launcher-sidebar")
        .w(move || px(appearance.sidebar_width()))
        .flex_shrink(0.0)
        .p(px(10.0))
        .gap_1()
        .bg(SURFACE)
        .text_color(INK)
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view().gap_1().children(
                    Page::MAIN
                        .iter()
                        .map(|&target| page_button(page, target, appearance).into_element()),
                ),
            ),
        )
        .child(download_status.hover(|style| style.bg(NAV_HOVER_BG)))
        .child(settings)
}

fn page_button(page: Signal<Page>, target: Page, appearance: WorkbenchAppearance) -> View {
    let selected = move || page.get() == target;
    let meta = target.meta();
    let button = view()
        .test_id(meta.test_id)
        .h(px(38.0))
        .rounded(px(7.0))
        .flex_row()
        .items_center()
        .px(move || px(if appearance.is_collapsed() { 4.0 } else { 8.0 }))
        .gap(move || px(if appearance.is_collapsed() { 0.0 } else { 8.0 }))
        .role(AccessibilityRole::Button)
        .label(meta.label)
        .accessibility_state(move || AccessibilityState {
            selected: selected(),
            current: selected().then_some(AccessibilityCurrent::Page),
            ..Default::default()
        })
        .bg(move || if selected() { NAV_ACTIVE_BG } else { SURFACE })
        .hover(|style| style.bg(NAV_HOVER_BG))
        .tooltip(styled_tooltip(
            Placement::Right,
            8.0,
            move || !appearance.is_collapsed(),
            meta.label,
        ))
        .child(view().w(px(3.0)).h(px(20.0)).rounded(px(2.0)).bg(move || {
            if selected() {
                ACCENT
            } else {
                AdaptiveColor::TRANSPARENT
            }
        }))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .flex_row()
                .items_center()
                .gap_3()
                .style_when(move || appearance.is_collapsed(), Styled::justify_center)
                .child(icon_svg(meta.icon, 20.0).text_color(move || {
                    if selected() {
                        NAV_ACTIVE_INK
                    } else {
                        MUTED_INK
                    }
                }))
                .child(
                    text(meta.label)
                        .text_size(px(14.0))
                        .text_color(move || if selected() { NAV_ACTIVE_INK } else { INK })
                        .style_when(selected, |style| style.font_weight(FontWeight::BOLD))
                        .style_when(move || appearance.is_collapsed(), Styled::hidden),
                ),
        );
    button.on_press(move || page.set(target))
}

fn panel<E: IntoElement>(
    page: Signal<Page>,
    target: Page,
    appearance: WorkbenchAppearance,
    content: impl FnOnce() -> E,
) -> Scroll {
    let body = content();
    scroll()
        .flex_1()
        .style_when(move || page.get() != target, Styled::hidden)
        .child(
            view()
                .w_full()
                .min_w(px(0.0))
                .p(move || {
                    px(if appearance.narrow_content() {
                        12.0
                    } else {
                        28.0
                    })
                })
                .gap_3()
                .child(
                    view().test_id("launcher-page-title").child(
                        text(target.label())
                            .text_size(px(24.0))
                            .font_weight(FontWeight::BOLD),
                    ),
                )
                .child(body.into_element()),
        )
}

fn home(
    page: Signal<Page>,
    appearance: WorkbenchAppearance,
    directory: &DirectoryControls,
    install: &Rc<InstallSession>,
    open_install: impl Fn() + Copy + 'static,
) -> View {
    let state = directory.state;
    view()
        .test_id("launcher-home-hero")
        .w_full()
        .min_w(px(0.0))
        .flex_row()
        .rounded(px(12.0))
        .overflow_hidden()
        .bg(AdaptiveColor::rgb(0x00e5_ecdb, 0x0025_3b32))
        .child(
            view()
                .test_id("launcher-home-copy")
                .w(move || px((appearance.content_width() * 0.42).clamp(236.0, 332.0)))
                .max_w(relative(1.0))
                .min_w(px(0.0))
                .flex_shrink(0.0)
                .min_h(move || {
                    px(if appearance.narrow_content() {
                        260.0
                    } else {
                        356.0
                    })
                })
                .p(move || {
                    px(if appearance.tight_content() {
                        10.0
                    } else if appearance.narrow_content() {
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
                        .text_size(move || {
                            px(if appearance.narrow_content() {
                                25.0
                            } else {
                                38.0
                            })
                        })
                        .font_weight(FontWeight::BOLD),
                )
                .child(
                    text(move || state.with(|state| state.message.clone()))
                        .w_full()
                        .min_w(px(0.0)),
                )
                .child(directory.home_actions(install, open_install))
                .child(button("管理游戏", move || page.set(Page::Game)))
                .child(install.home_status()),
        )
        .child(
            image(HOME_LANDSCAPE.as_str())
                .test_id("launcher-home-landscape")
                .flex_1()
                .min_w(px(0.0))
                .object_fit(ResizeMode::Cover),
        )
}

fn launch_directory(
    environment: &dyn LauncherEnvironment,
    state: Signal<InstallationState>,
) -> Option<PathBuf> {
    let path = state.with_untracked(|state| state.launch_directory().map(Path::to_path_buf))?;
    match environment.validate(&path) {
        Ok(directory) => Some(directory),
        Err(error) => {
            state.update(|state| {
                state.directories.retain(|directory| directory != &path);
                state.candidate = None;
                state.saved = Some(SavedDirectory::Invalid(path));
                state.message = format!("启动失败：目录已失效：{error}。请重新选择游戏目录。");
            });
            None
        }
    }
}

fn report_launch<T>(state: Signal<InstallationState>, result: Result<T, String>) -> Result<T, ()> {
    let message = match &result {
        Ok(_) => "已请求启动游戏".into(),
        Err(error) => format!("启动失败：{error}"),
    };
    state.update(|state| state.message = message);
    result.map_err(|_| ())
}

#[derive(Clone)]
struct DirectoryControls {
    environment: Rc<dyn LauncherEnvironment>,
    state: Signal<InstallationState>,
    picking: Signal<bool>,
    #[cfg(target_os = "windows")]
    scanner: Option<Rc<crate::scanner::Sync>>,
}

impl DirectoryControls {
    fn home_actions(
        &self,
        install: &Rc<InstallSession>,
        open_install: impl Fn() + Copy + 'static,
    ) -> View {
        let state = self.state;
        let controls = self.clone();
        let ready = move || state.with(|s| s.launch_directory().is_some());
        let picker = install.clone();
        let supports_install = self.environment.supports_install();
        view()
            .w_full()
            .min_w(px(0.0))
            .items_start()
            .child(
                action_button("普通启动", move || controls.launch())
                    .test_id("launcher-ordinary-launch")
                    .bg(ACCENT)
                    .text_color(white())
                    .px_4()
                    .py_3()
                    .style_when(move || !ready(), Styled::hidden),
            )
            .child({
                #[cfg(target_os = "windows")]
                {
                    self.scanner.as_ref().map(crate::scanner::Sync::status)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    None::<View>
                }
            })
            .child(
                self.candidates(Some("launcher-home-candidate-path"))
                    .style_when(ready, Styled::hidden),
            )
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

    fn launch(&self) {
        #[cfg(target_os = "windows")]
        if self
            .scanner
            .as_ref()
            .is_some_and(|scanner| scanner.is_active())
        {
            return;
        }
        let environment = self.environment.clone();
        let state = self.state;
        let Some(directory) = launch_directory(environment.as_ref(), state) else {
            return;
        };
        #[cfg(target_os = "windows")]
        if let Some(scanner) = &self.scanner {
            state.update(|state| state.message = "正在准备启动游戏".into());
            scanner.launch(move || {
                let path = launch_directory(environment.as_ref(), state).ok_or(())?;
                let result = if path == directory {
                    environment.launch_with_process(&path)
                } else {
                    Err("等待期间游戏目录已更换，请重新启动".into())
                };
                report_launch(state, result)
            });
            return;
        }
        let _ = report_launch(state, environment.launch(&directory));
    }

    fn candidates(&self, path_test_id: Option<&'static str>) -> View {
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
                    let mut path = text(only.display().to_string()).w_full().min_w(px(0.0));
                    if let Some(id) = path_test_id {
                        path = path.test_id(id);
                    }
                    return vec![path.into_element()];
                }
                directories
                    .into_iter()
                    .map(|path| {
                        let selected_path = path.clone();
                        action_button(format!("选择：{}", path.display()), move || {
                            state.update(|s| s.candidate = Some(path.clone()));
                        })
                        .w_full()
                        .min_w(px(0.0))
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
            .child(
                text(move || {
                    state.with(|s| match &s.saved {
                        Some(SavedDirectory::Valid(path)) => {
                            format!("默认目录：{}", path.display())
                        }
                        Some(SavedDirectory::Invalid(path)) => {
                            format!("已失效的默认目录：{}", path.display())
                        }
                        None => "尚未选择默认目录".into(),
                    })
                })
                .w_full()
                .min_w(px(0.0)),
            )
            .child(
                text(move || state.with(|s| s.message.clone()))
                    .w_full()
                    .min_w(px(0.0)),
            )
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
                    .child(self.candidates(None))
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
            ..
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
