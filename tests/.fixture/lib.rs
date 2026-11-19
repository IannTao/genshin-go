use nsh_app_genshin_go::accounts::{AccountOperation, Authorization, QrSession, QrStatus};
use nsh_app_genshin_go::launcher::{DirectoryChoice, LauncherEnvironment, launcher};
use nsh_app_genshin_go::profile::{Profile, VerifiedAccount};
use nsh_core::prelude::*;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
};

struct Environment {
    profile: RefCell<Profile>,
    revision: Cell<u64>,
    uid_lookup: Signal<u32>,
    qr_status: Signal<u32>,
    slow_qr_creation: Signal<bool>,
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
    fn load_profile(&self) -> Result<Profile, String> {
        Ok(self.profile.borrow().clone())
    }
    fn select_uid(&self, uid: &str, create: bool) -> Result<Profile, String> {
        let mut profile = self.profile.borrow_mut();
        if create && !profile.uids.iter().any(|saved| saved == uid) {
            profile.uids.push(uid.into());
        }
        profile.active_uid = Some(uid.into());
        Ok(profile.clone())
    }
    fn delete_uid_history(&self, uid: &str) -> Result<Profile, String> {
        let profile = self.profile.borrow();
        if !profile.uids.iter().any(|saved| saved == uid) {
            return Err("该 UID 数据空间不存在".into());
        }
        Ok(profile.clone())
    }
    fn verify_uid(&self, _authorization: Authorization) -> AccountOperation<String> {
        let lookup = self.uid_lookup.get_untracked();
        Box::pin(async move {
            match lookup {
                1 => Err("未查到该米哈游账号的游戏 UID".into()),
                2 => Ok("987654321".into()),
                _ => Ok("123456789".into()),
            }
        })
    }
    fn save_authorization(
        &self,
        authorization: Authorization,
        uid: &str,
        expected: Option<&VerifiedAccount>,
    ) -> Result<Profile, String> {
        if self.profile.borrow().account.as_ref() != expected {
            return Err("本机授权已在另一启动器中更改".into());
        }
        let mut profile = self.profile.borrow_mut();
        if !profile.uids.iter().any(|saved| saved == uid) {
            profile.uids.push(uid.into());
        }
        profile.active_uid.get_or_insert_with(|| uid.into());
        let revision = self.revision.get() + 1;
        self.revision.set(revision);
        profile.account = Some(VerifiedAccount {
            account_id: authorization.account_id,
            uid: uid.into(),
            revision: revision.to_string(),
        });
        Ok(profile.clone())
    }
    fn remove_authorization(&self, account: &VerifiedAccount) -> Result<Profile, String> {
        let mut profile = self.profile.borrow_mut();
        if profile.account.as_ref() != Some(account) {
            return Err("授权已更改".into());
        }
        profile.account = None;
        Ok(profile.clone())
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
        let slow = self.slow_qr_creation.get_untracked();
        Box::pin(async move {
            if slow {
                nsh_core::element::timer(std::time::Duration::from_secs(1)).await;
            }
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
        profile: RefCell::new(Profile::default()),
        revision: Cell::new(0),
        uid_lookup: signal(0),
        qr_status: signal(0),
        slow_qr_creation: signal(false),
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
    let other_launcher = environment.clone();
    let qr_status = environment.qr_status;
    let uid_lookup = environment.uid_lookup;
    let slow_qr_creation = environment.slow_qr_creation;
    let next_account = environment.next_account;
    let fail_save = environment.fail_save;
    let fail_launch = environment.fail_launch;
    let launched = environment.launched;
    let generation = signal(0_u32);
    view()
        .flex_1()
        .child(
            view()
                .on_press(move || uid_lookup.set(1))
                .child(text("模拟查不到游戏 UID")),
        )
        .child(
            view()
                .on_press(move || uid_lookup.set(0))
                .child(text("模拟查到唯一游戏 UID")),
        )
        .child(
            view()
                .on_press(move || uid_lookup.set(2))
                .child(text("模拟归属 UID 改变")),
        )
        .child(
            view()
                .on_press(move || slow_qr_creation.set(true))
                .child(text("模拟延迟创建二维码")),
        )
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
                .on_press(move || {
                    assert!(
                        other_launcher
                            .save_authorization(
                                Authorization {
                                    account_id: "10002".into(),
                                    cookie_header: "fixture-cookie".into(),
                                },
                                "987654321",
                                None,
                            )
                            .is_ok()
                    );
                })
                .child(text("模拟另一启动器先保存授权")),
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
