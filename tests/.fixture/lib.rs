use nsh_app_genshin_go::accounts::{AccountOperation, Authorization, QrSession, QrStatus};
use nsh_app_genshin_go::launcher::{DirectoryChoice, LauncherEnvironment, launcher};
use nsh_core::prelude::*;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

struct Environment {
    accounts: RefCell<Vec<String>>,
    qr_status: Signal<u32>,
    next_account: Signal<u32>,
    selected: RefCell<Option<PathBuf>>,
    directories: RefCell<Vec<PathBuf>>,
    fail_save: Signal<bool>,
    fail_launch: Signal<bool>,
    launched: Signal<String>,
}

impl LauncherEnvironment for Environment {
    fn check_authorization(&self, _account_id: String) -> AccountOperation<bool> {
        Box::pin(async { Ok(false) })
    }
    fn load_accounts(&self) -> Result<Vec<String>, String> {
        Ok(self.accounts.borrow().clone())
    }
    fn save_authorization(&self, authorization: Authorization) -> Result<Vec<String>, String> {
        let mut accounts = self.accounts.borrow_mut();
        if !accounts.contains(&authorization.account_id) {
            accounts.push(authorization.account_id);
        }
        Ok(accounts.clone())
    }
    fn poll_qr(&self, _qr: QrSession) -> AccountOperation<QrStatus> {
        let status = self.qr_status.get_untracked();
        let account = self.next_account.get_untracked();
        Box::pin(async move {
            match status {
                1 => Ok(QrStatus::Scanned),
                2 => Ok(QrStatus::Confirmed(Authorization {
                    account_id: format!("1000{account}"),
                    cookie_header: "account_id_v2=10001; ltmid_v2=20001; ltoken_v2=fixture-secret; cookie_token_v2=fixture-cookie".into(),
                })),
                3 => Ok(QrStatus::Expired),
                4 => Ok(QrStatus::Cancelled),
                5 => Err("网络失败：无法连接米哈游服务，请重试".into()),
                6 => Err("扫码服务拒绝请求：设备或网络环境存在风险（-3503）".into()),
                _ => Ok(QrStatus::Waiting),
            }
        })
    }
    fn create_qr(
        &self,
    ) -> nsh_app_genshin_go::accounts::AccountOperation<nsh_app_genshin_go::accounts::QrSession>
    {
        self.qr_status.set(0);
        Box::pin(async {
            Ok(nsh_app_genshin_go::accounts::QrSession {
                url: "https://user.mihoyo.com/fixture".into(),
                ticket: "fixture".into(),
                device: "fixture".into(),
            })
        })
    }
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
        accounts: RefCell::new(Vec::new()),
        qr_status: signal(0),
        next_account: signal(1),
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
    let qr_status = environment.qr_status;
    let next_account = environment.next_account;
    let fail_save = environment.fail_save;
    let fail_launch = environment.fail_launch;
    let launched = environment.launched;
    let generation = signal(0_u32);
    view()
        .flex_1()
        .child(
            view()
                .on_press(move || qr_status.set(2))
                .child(text("模拟扫码确认")),
        )
        .child(
            view()
                .on_press(move || qr_status.set(1))
                .child(text("模拟已扫描")),
        )
        .child(
            view()
                .on_press(move || qr_status.set(3))
                .child(text("模拟二维码过期")),
        )
        .child(
            view()
                .on_press(move || qr_status.set(4))
                .child(text("模拟手机取消")),
        )
        .child(
            view()
                .on_press(move || qr_status.set(5))
                .child(text("模拟扫码网络失败")),
        )
        .child(
            view()
                .on_press(move || qr_status.set(6))
                .child(text("模拟服务风控")),
        )
        .child(
            view()
                .on_press(move || next_account.set(2))
                .child(text("模拟另一米哈游账号")),
        )
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
