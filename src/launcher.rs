//! Application boundary shared by the native root and the isolated test host.

use nsh_base::theme;
use nsh_core::element::input;
use nsh_core::prelude::*;
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    rc::Rc,
};

pub type DirectoryChoice = Pin<Box<dyn Future<Output = Result<Option<PathBuf>, String>>>>;

/// External operations needed by installation selection and ordinary launch.
pub trait LauncherEnvironment {
    fn discover(&self) -> Result<Vec<PathBuf>, String>;
    fn load_default(&self) -> Result<Option<PathBuf>, String>;
    fn save_default(&self, directory: &Path) -> Result<(), String>;
    fn validate(&self, directory: &Path) -> Result<PathBuf, String>;
    fn launch(&self, directory: &Path) -> Result<(), String>;
    fn choose_directory(&self) -> DirectoryChoice;
}

#[derive(Clone, PartialEq)]
struct InstallationState {
    directories: Vec<PathBuf>,
    selected: Option<PathBuf>,
    message: String,
}

fn initial_state(environment: &dyn LauncherEnvironment) -> InstallationState {
    let mut state = InstallationState {
        directories: Vec::new(),
        selected: None,
        message: String::new(),
    };
    match environment.discover() {
        Ok(paths) => state.directories = paths,
        Err(error) => state.message = format!("自动查找失败：{error}。请选择游戏目录。"),
    }
    match environment.load_default() {
        Ok(Some(directory)) => match environment.validate(&directory) {
            Ok(directory) => {
                if !state.directories.contains(&directory) {
                    state.directories.push(directory.clone());
                }
                state.selected = Some(directory);
            }
            Err(error) => state.message = format!("默认目录已失效：{error}。请重新选择游戏目录。"),
        },
        Ok(None) => {}
        Err(error) => state.message = format!("读取默认目录失败：{error}。请重新选择游戏目录。"),
    }
    if state.message.is_empty() {
        state.message = if state.selected.is_some() {
            "默认目录已载入"
        } else if state.directories.is_empty() {
            "未找到游戏，请先安装国服官服，再选择游戏目录。"
        } else {
            "请选择游戏目录并设为默认目录。"
        }
        .into();
    }
    state
}

fn select(environment: &dyn LauncherEnvironment, state: Signal<InstallationState>, path: &Path) {
    let result = environment.validate(path).and_then(|directory| {
        environment.save_default(&directory)?;
        Ok(directory)
    });
    match result {
        Ok(directory) => state.update(|s| {
            if !s.directories.contains(&directory) {
                s.directories.push(directory.clone());
            }
            s.selected = Some(directory);
            s.message = "默认目录已保存".into();
        }),
        Err(error) => state.update(|s| s.message = format!("无法选择目录：{error}")),
    }
}

fn button(label: impl Into<String>, action: impl Fn() + 'static) -> impl IntoElement {
    view()
        .px_4()
        .py_3()
        .rounded(px(8.0))
        .bg(move || theme().secondary)
        .role(nsh_core::AccessibilityRole::Button)
        .on_press(action)
        .child(text(label.into()))
}

/// Renders the same launcher with either the Windows environment or a test host.
pub fn launcher(environment: Rc<dyn LauncherEnvironment>) -> impl IntoElement {
    let state = signal(initial_state(environment.as_ref()));
    let manual = signal(String::new());
    let picking = signal(false);
    let choose_environment = environment.clone();
    let path_environment = environment.clone();
    let refresh_environment = environment.clone();
    let launch_environment = environment.clone();
    view()
        .flex_1()
        .p_6()
        .gap_4()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(text("原神启动器").text_size(px(28.0)))
        .child(text("Windows · 国服官服"))
        .child(text(move || state.get().message))
        .child(text(move || {
            state
                .get()
                .selected
                .map(|p| format!("默认目录：{}", p.display()))
                .unwrap_or_else(|| "尚未选择默认目录".into())
        }))
        .child(
            scroll()
                .flex_1()
                .child(view().gap_2().child(dynamic(move || {
                    state
                        .get()
                        .directories
                        .into_iter()
                        .map(|path| {
                            let environment = environment.clone();
                            button(format!("选择：{}", path.display()), move || {
                                select(environment.as_ref(), state, &path)
                            })
                            .into_element()
                        })
                        .collect::<Vec<_>>()
                }))),
        )
        .child(button("重新查找", move || {
            state.set(initial_state(refresh_environment.as_ref()))
        }))
        .child(
            input()
                .test_id("game-directory")
                .label("游戏目录")
                .h_12()
                .px_3()
                .bg(move || theme().secondary)
                .placeholder("游戏目录（含 YuanShen.exe）")
                .value(move || manual.get())
                .on_change(move |value: &str| manual.set(value.to_owned())),
        )
        .child(button("使用此目录", move || {
            select(
                path_environment.as_ref(),
                state,
                Path::new(manual.get().trim().trim_matches('"')),
            )
        }))
        .child(button("选择游戏目录", move || {
            if picking.get_untracked() {
                return;
            }
            picking.set(true);
            let environment = choose_environment.clone();
            spawn(async move {
                match environment.choose_directory().await {
                    Ok(Some(path)) => select(environment.as_ref(), state, &path),
                    Ok(None) => {}
                    Err(error) => state.update(|s| {
                        s.message = format!("目录选择失败：{error}。也可在上方输入路径。")
                    }),
                }
                picking.set(false);
            });
        }))
        .child(button("普通启动", move || {
            let selected = state.get_untracked().selected;
            let result = selected
                .ok_or_else(|| "请先选择有效游戏目录".to_owned())
                .and_then(|path| {
                    let directory = launch_environment
                        .validate(&path)
                        .map_err(|error| format!("目录已失效：{error}。请重新选择游戏目录。"))?;
                    launch_environment.launch(&directory)
                });
            state.update(|s| {
                s.message = match result {
                    Ok(()) => "已请求启动游戏".into(),
                    Err(error) => format!("启动失败：{error}"),
                }
            });
        }))
}
