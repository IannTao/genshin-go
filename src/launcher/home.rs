use super::{
    ACCENT, CONTROL, INK, InstallationState, LauncherEnvironment, Page, SavedDirectory,
    WorkbenchAppearance, action_button, action_button_with, button, find_installations, select,
};
use crate::game_install::InstallSession;
use base64::{Engine, engine::general_purpose::STANDARD};
use nsh_core::element::input;
use nsh_core::prelude::*;
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::LazyLock,
};

static HOME_LANDSCAPE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(include_bytes!("../../assets/home-landscape.jpg"))
    )
});

pub(super) fn game_management(
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

pub(super) fn home(
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
pub(super) struct DirectoryControls {
    environment: Rc<dyn LauncherEnvironment>,
    state: Signal<InstallationState>,
    picking: Signal<bool>,
    #[cfg(target_os = "windows")]
    scanner: Option<Rc<crate::scanner::Sync>>,
}

impl DirectoryControls {
    pub(super) fn new(
        environment: Rc<dyn LauncherEnvironment>,
        state: Signal<InstallationState>,
        #[cfg(target_os = "windows")] scanner: Option<Rc<crate::scanner::Sync>>,
    ) -> Self {
        Self {
            environment,
            state,
            picking: signal(false),
            #[cfg(target_os = "windows")]
            scanner,
        }
    }

    fn home_actions(
        &self,
        install: &Rc<InstallSession>,
        open_install: impl Fn() + Copy + 'static,
    ) -> View {
        let state = self.state;
        let controls = self.clone();
        let ready = move || state.with(|s| s.launch_directory().is_some());
        let ready_untracked = move || state.with_untracked(|s| s.launch_directory().is_some());
        let picker = install.clone();
        let supports_install = self.environment.supports_install();
        let empty_home =
            move || !ready_untracked() && state.with_untracked(|s| s.directories.is_empty());
        view()
            .w_full()
            .min_w(px(0.0))
            .items_start()
            .child(crate::keyboard::with_scope(ready_untracked, move || {
                action_button("普通启动", move || controls.launch())
                    .test_id("launcher-ordinary-launch")
                    .bg(ACCENT)
                    .text_color(white())
                    .px_4()
                    .py_3()
                    .style_when(move || !ready(), Styled::hidden)
            }))
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
            .child(crate::keyboard::with_scope(
                move || !ready_untracked(),
                || {
                    self.candidates(Some("launcher-home-candidate-path"))
                        .style_when(ready, Styled::hidden)
                },
            ))
            .child(crate::keyboard::with_scope(empty_home, || {
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
                    }))
            }))
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
                action_button_with(
                    "确认保存目录",
                    move || {
                        if let Some(path) =
                            state.with_untracked(|s| s.chosen_candidate().map(Path::to_path_buf))
                        {
                            select(environment.as_ref(), state, &path);
                        }
                    },
                    Some(Rc::new(move || {
                        state.with_untracked(|s| s.chosen_candidate().is_some())
                    })),
                )
                .0
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
            .child(crate::keyboard::with_scope(
                move || state.with_untracked(|s| s.editing),
                || {
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
                        .child(crate::keyboard::register_input(
                            input()
                                .test_id("game-directory")
                                .label("游戏目录")
                                .h_10()
                                .px_3()
                                .bg(CONTROL)
                                .placeholder("游戏目录（含 YuanShen.exe）")
                                .value(move || manual.get())
                                .on_change(move |value: &str| manual.set(value.to_owned())),
                        ))
                        .child(button("使用此目录", move || {
                            select(
                                path_environment.as_ref(),
                                state,
                                Path::new(manual.get().trim().trim_matches('"')),
                            );
                        }))
                },
            ))
    }

    fn directory_picker(&self, label: &'static str) -> impl IntoElement {
        let Self {
            environment,
            state,
            picking,
            ..
        } = self.clone();
        action_button_with(
            label,
            move || {
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
                            s.message =
                                format!("目录选择失败：{error}。可在游戏管理页手动输入路径。");
                        }),
                    }
                    picking.set(false);
                });
            },
            Some(Rc::new(move || !picking.get_untracked())),
        )
        .0
        .bg(CONTROL)
        .text_color(INK)
        .disabled(move || picking.get())
    }
}
