use nsh_app_genshin_go::launcher::{DirectoryChoice, LauncherEnvironment, launcher};
use nsh_core::prelude::*;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

struct Environment {
    selected: RefCell<Option<PathBuf>>,
    directories: RefCell<Vec<PathBuf>>,
    fail_save: Signal<bool>,
    fail_launch: Signal<bool>,
    launched: Signal<String>,
}

impl LauncherEnvironment for Environment {
    fn discover(&self) -> Result<Vec<PathBuf>, String> {
        Ok(self
            .directories
            .borrow()
            .iter()
            .filter(|p| p.as_path() != Path::new("E:/手动目录"))
            .cloned()
            .collect())
    }
    fn load_default(&self) -> Result<Option<PathBuf>, String> {
        Ok(self.selected.borrow().clone())
    }
    fn save_default(&self, path: &Path) -> Result<(), String> {
        if self.fail_save.get_untracked() {
            return Err("配置不可写".into());
        }
        self.selected.replace(Some(path.into()));
        Ok(())
    }
    fn validate(&self, path: &Path) -> Result<PathBuf, String> {
        if self.directories.borrow().contains(&path.to_path_buf()) {
            Ok(path.into())
        } else {
            Err("不是国服官服游戏目录".into())
        }
    }
    fn launch(&self, path: &Path) -> Result<(), String> {
        if self.fail_launch.get_untracked() {
            return Err("系统拒绝启动".into());
        }
        self.launched.set(format!("请求启动：{}", path.display()));
        Ok(())
    }
    fn choose_directory(&self) -> DirectoryChoice {
        Box::pin(async { Ok(Some("E:/手动目录".into())) })
    }
}

#[native_module("genshin-go/fixture")]
fn native(registry: &mut Registry) {
    registry.register::<Fixture>();
}

/// The isolated application environment for launcher interaction tests.
#[component]
pub fn Fixture() -> impl IntoElement {
    let environment = Rc::new(Environment {
        selected: RefCell::new(None),
        directories: RefCell::new(vec![
            "C:/原神".into(),
            "D:/原神".into(),
            "E:/手动目录".into(),
        ]),
        fail_save: signal(false),
        fail_launch: signal(false),
        launched: signal(String::new()),
    });
    let remove_environment = environment.clone();
    let fail_save = environment.fail_save;
    let fail_launch = environment.fail_launch;
    let launched = environment.launched;
    let generation = signal(0_u32);
    view()
        .flex_1()
        .child(
            view()
                .on_press(move || generation.update(|g| *g += 1))
                .child(text("重新打开启动器")),
        )
        .child(
            view()
                .on_press(move || remove_environment.directories.borrow_mut().clear())
                .child(text("模拟安装已移除")),
        )
        .child(
            view()
                .on_press(move || fail_save.set(true))
                .child(text("模拟配置写入失败")),
        )
        .child(
            view()
                .on_press(move || fail_launch.set(true))
                .child(text("模拟系统启动失败")),
        )
        .child(text(move || launched.get()))
        .child(dynamic(move || {
            view()
                .key(format!("session-{}", generation.get()))
                .flex_1()
                .child(launcher(environment.clone()))
        }))
}
