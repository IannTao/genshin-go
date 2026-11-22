use nsh_app_genshin_go::accounts::{AccountOperation, Authorization, QrSession, QrStatus};
use nsh_app_genshin_go::community_sign_in::{Clock, Completion, Failure, Operation, SignInResult};
use nsh_app_genshin_go::game_install::{InstallControl, InstallOperation};
use nsh_app_genshin_go::launcher::{DirectoryChoice, LauncherEnvironment, launcher};
use nsh_app_genshin_go::profile::{Profile, VerifiedAccount};
use nsh_core::prelude::*;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

const CONFIRMED_WITH_NEW_COOKIE: u32 = 7;
const RENEWED_COOKIE: &str = "fixture-renewed-cookie";

#[cfg(target_os = "windows")]
mod scanner;

enum DownloadOutcome {
    Interrupted,
    Complete,
}

fn claimed(completion: Completion) -> SignInResult {
    SignInResult {
        completion,
        reward_claimed: true,
        reward_points: Some(30),
    }
}

struct Environment {
    #[cfg(target_os = "windows")]
    scanner: Rc<RefCell<Option<scanner::Scanner>>>,
    #[cfg(target_os = "windows")]
    game_running: Signal<bool>,
    launches: Signal<u32>,
    profile: RefCell<Profile>,
    revision: Cell<u64>,
    cookie: RefCell<Option<String>>,
    sign_in_day: Signal<u64>,
    suspended_seconds: Signal<u64>,
    sign_in_reply: Signal<Result<SignInResult, Failure>>,
    sign_in_attempts: Signal<u32>,
    uid_lookup: Signal<u32>,
    qr_status: Signal<u32>,
    slow_qr_creation: Signal<bool>,
    next_account: Signal<u32>,
    selected: Rc<RefCell<Option<PathBuf>>>,
    directories: RefCell<Vec<PathBuf>>,
    fail_save: Signal<bool>,
    fail_authorization_save: Signal<bool>,
    fail_launch: Signal<bool>,
    fail_plugin_catalog: Signal<bool>,
    plugins_dir: PathBuf,
    launched: Signal<String>,
    install_target: Rc<RefCell<Option<PathBuf>>>,
    install_progress: Rc<Cell<u64>>,
    install_outcome: Rc<Cell<Option<DownloadOutcome>>>,
    previous_install: RefCell<Option<InstallControl>>,
    installed: Rc<RefCell<Option<PathBuf>>>,
}

impl LauncherEnvironment for Environment {
    #[cfg(target_os = "windows")]
    fn uid_database_path(&self) -> Option<Result<PathBuf, String>> {
        self.scanner.borrow().as_ref()?.local.uid_database_path()
    }
    #[cfg(target_os = "windows")]
    fn launch_with_process(
        &self,
        directory: &Path,
    ) -> Result<Option<nsh_app_genshin_go::launcher::GameProcess>, String> {
        self.launch(directory)?;
        let running = self.game_running;
        running.set(true);
        Ok(Some(Box::new(move || running.get_untracked())))
    }
    #[cfg(target_os = "windows")]
    fn start_scanner(&self, _program: &Path) -> nsh_app_genshin_go::launcher::ScannerLaunch {
        self.scanner.borrow().as_ref().unwrap().launch()
    }
    fn community_clock(&self) -> Option<Clock> {
        Some(Clock {
            day: self.sign_in_day.get_untracked(),
            suspended: Duration::from_secs(self.suspended_seconds.get_untracked()),
        })
    }
    fn community_sign_in(
        &self,
        account: VerifiedAccount,
        day: u64,
        allow_submission: bool,
    ) -> Operation {
        let current = self.profile.borrow().account.clone();
        let current_day = self.sign_in_day.get_untracked();
        let result = self.sign_in_reply.get_untracked();
        self.sign_in_attempts.update(|count| *count += 1);
        Box::pin(async move {
            nsh_core::element::timer(Duration::from_secs(1)).await;
            assert_eq!(current.as_ref(), Some(&account));
            assert_eq!(day, current_day);
            match result {
                Ok(result) if result.completion == Completion::Signed && !allow_submission => {
                    Err(Failure::Other("米游社尚未确认签到结果".into()))
                }
                result => result,
            }
        })
    }
    fn check_authorization(&self, account_id: String) -> AccountOperation<bool> {
        let matches_account = self
            .profile
            .borrow()
            .account
            .as_ref()
            .is_some_and(|account| account.account_id == account_id);
        let valid = self
            .cookie
            .borrow()
            .as_deref()
            .filter(|_| matches_account)
            .map(|cookie| cookie == RENEWED_COOKIE);
        Box::pin(async move { valid.ok_or_else(|| "账号授权不存在，请重新扫码".into()) })
    }
    fn load_profile(&self) -> Result<Profile, String> {
        #[cfg(target_os = "windows")]
        if let Some(scanner) = self.scanner.borrow().as_ref() {
            return scanner.local.load_profile();
        }
        Ok(self.profile.borrow().clone())
    }
    fn select_uid(&self, uid: &str, create: bool) -> Result<Profile, String> {
        #[cfg(target_os = "windows")]
        if let Some(scanner) = self.scanner.borrow().as_ref() {
            return scanner.local.select_uid(uid, create);
        }
        let mut profile = self.profile.borrow_mut();
        if create && !profile.uids.iter().any(|saved| saved == uid) {
            profile.uids.push(uid.into());
        }
        profile.active_uid = Some(uid.into());
        Ok(profile.clone())
    }
    fn delete_uid_history(&self, uid: &str) -> Result<Profile, String> {
        #[cfg(target_os = "windows")]
        if let Some(scanner) = self.scanner.borrow().as_ref() {
            return scanner.local.delete_uid_history(uid);
        }
        let profile = self.profile.borrow();
        if !profile.uids.iter().any(|saved| saved == uid) {
            return Err("该 UID 数据空间不存在".into());
        }
        Ok(profile.clone())
    }
    fn verify_uid(&self, authorization: Authorization) -> AccountOperation<String> {
        let lookup = self.uid_lookup.get_untracked();
        Box::pin(async move {
            match lookup {
                1 => Err("未查到该米哈游账号的游戏 UID".into()),
                2 => Ok("987654321".into()),
                _ if authorization.account_id == "10002" => Ok("987654321".into()),
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
        if self.fail_authorization_save.get_untracked() {
            self.fail_authorization_save.set(false);
            return Err("授权数据库不可写".into());
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
        self.cookie.replace(Some(authorization.cookie_header));
        Ok(profile.clone())
    }
    fn remove_authorization(&self, account: &VerifiedAccount) -> Result<Profile, String> {
        let mut profile = self.profile.borrow_mut();
        if profile.account.as_ref() != Some(account) {
            return Err("授权已更改".into());
        }
        profile.account = None;
        self.cookie.replace(None);
        Ok(profile.clone())
    }
    fn poll_qr(&self, _qr: QrSession) -> AccountOperation<QrStatus> {
        let status = self.qr_status.get_untracked();
        let account = self.next_account.get_untracked();
        Box::pin(async move {
            match status {
                1 => Ok(QrStatus::Scanned),
                2 | CONFIRMED_WITH_NEW_COOKIE => Ok(QrStatus::Confirmed(Authorization {
                    account_id: format!("1000{account}"),
                    cookie_header: if status == CONFIRMED_WITH_NEW_COOKIE {
                        RENEWED_COOKIE
                    } else {
                        "fixture-original-cookie"
                    }
                    .into(),
                    community_cookie: None,
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
        if self.directories.borrow().contains(&path.to_path_buf())
            || self.installed.borrow().as_deref() == Some(path)
        {
            Ok(path.into())
        } else {
            Err("不是国服官服游戏目录".into())
        }
    }
    fn launch(&self, path: &Path) -> Result<(), String> {
        self.launches.update(|n| *n += 1);
        if self.fail_launch.get_untracked() {
            return Err("系统拒绝启动".into());
        }
        self.launched.set(format!("请求启动：{}", path.display()));
        Ok(())
    }
    fn choose_directory(&self) -> DirectoryChoice {
        Box::pin(async { Ok(Some("E:/手动目录".into())) })
    }
    fn choose_install_parent(&self) -> DirectoryChoice {
        Box::pin(async { Ok(Some("F:/新安装位置".into())) })
    }
    fn supports_install(&self) -> bool {
        true
    }
    fn pending_install(&self) -> Result<Option<PathBuf>, String> {
        Ok(self.install_target.borrow().clone())
    }
    fn load_plugin_catalog(
        &self,
    ) -> Result<nsh_app_genshin_go::plugins::PluginCatalog, String> {
        if self.fail_plugin_catalog.get_untracked() {
            return Err("精选插件目录清单已损坏".into());
        }
        nsh_app_genshin_go::plugins::bundled_catalog()
    }
    fn plugins_root(&self) -> Option<PathBuf> {
        Some(self.plugins_dir.clone())
    }
    fn install_game(&self, parent: Option<PathBuf>, control: InstallControl) -> InstallOperation {
        if self
            .previous_install
            .borrow()
            .as_ref()
            .is_some_and(|previous| !previous.is_paused())
        {
            return Box::pin(async { Err("上次下载仍在运行".into()) });
        }
        if parent.is_some() == self.install_target.borrow().is_some() {
            return Box::pin(async { Err("下载目标与保存的任务不一致".into()) });
        }
        self.previous_install.replace(Some(control.clone()));
        if let Some(parent) = parent {
            self.install_target
                .replace(Some(parent.join("Genshin Impact Game")));
            self.install_progress.set(1);
        }
        let progress = self.install_progress.clone();
        let outcome = self.install_outcome.clone();
        let pending = self.install_target.clone();
        let installed = self.installed.clone();
        let selected = self.selected.clone();
        Box::pin(async move {
            while !control.is_paused() {
                control.report(progress.get(), 100);
                match outcome.take() {
                    Some(DownloadOutcome::Interrupted) => return Err("下载连接断开".into()),
                    Some(DownloadOutcome::Complete) => {
                        let path = pending.take();
                        installed.replace(path.clone());
                        selected.replace(path.clone());
                        return Ok(path);
                    }
                    None => nsh_core::element::timer(Duration::from_millis(200)).await,
                }
            }
            Ok(None)
        })
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
        #[cfg(target_os = "windows")]
        scanner: Rc::default(),
        #[cfg(target_os = "windows")]
        game_running: signal(false),
        launches: signal(0),
        profile: RefCell::new(Profile::default()),
        revision: Cell::new(0),
        cookie: RefCell::new(None),
        sign_in_day: signal(1),
        suspended_seconds: signal(0),
        sign_in_reply: signal(Ok(claimed(Completion::AlreadySigned))),
        sign_in_attempts: signal(0),
        uid_lookup: signal(0),
        qr_status: signal(0),
        slow_qr_creation: signal(false),
        next_account: signal(1),
        selected: Rc::default(),
        directories: RefCell::new(vec![
            "C:/原神".into(),
            "D:/原神".into(),
            "E:/手动目录".into(),
        ]),
        fail_save: signal(false),
        fail_authorization_save: signal(false),
        fail_launch: signal(false),
        fail_plugin_catalog: signal(false),
        plugins_dir: std::env::temp_dir().join(format!(
            "genshin-go-fixture-plugins-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        )),
        launched: signal(String::new()),
        install_target: Rc::default(),
        install_progress: Rc::default(),
        install_outcome: Rc::default(),
        previous_install: RefCell::new(None),
        installed: Rc::default(),
    });
    let remove_environment = environment.clone();
    let undiscoverable_environment = environment.clone();
    let single_environment = environment.clone();
    let move_environment = environment.clone();
    let other_launcher = environment.clone();
    let qr_status = environment.qr_status;
    let uid_lookup = environment.uid_lookup;
    let slow_qr_creation = environment.slow_qr_creation;
    let next_account = environment.next_account;
    let fail_save = environment.fail_save;
    let fail_authorization_save = environment.fail_authorization_save;
    let fail_launch = environment.fail_launch;
    let fail_plugin_catalog = environment.fail_plugin_catalog;
    let launched = environment.launched;
    let generation = signal(0_u32);
    #[cfg(target_os = "windows")]
    let scanner = Some(scanner::controls(
        environment.scanner.clone(),
        generation,
        environment.launches,
        environment.game_running,
    ));
    #[cfg(not(target_os = "windows"))]
    let scanner = None::<View>;
    let sign_in_day = environment.sign_in_day;
    let suspended_seconds = environment.suspended_seconds;
    let sign_in_reply = environment.sign_in_reply;
    let sign_in_attempts = environment.sign_in_attempts;
    let install_progress = environment.install_progress.clone();
    let interrupted_install = environment.install_outcome.clone();
    let completed_install = environment.install_outcome.clone();
    view()
        .flex_1()
        .child(scanner)
        .child(
            view()
                .on_press(move || sign_in_day.update(|day| *day += 1))
                .child(text("模拟签到跨日")),
        )
        .child(
            view()
                .on_press(move || suspended_seconds.update(|seconds| *seconds += 10))
                .child(text("模拟休眠恢复")),
        )
        .child(
            view()
                .on_press(move || sign_in_reply.set(Err(Failure::Other("无法连接米游社".into()))))
                .child(text("模拟签到网络失败")),
        )
        .child(
            view()
                .on_press(move || sign_in_reply.set(Ok(claimed(Completion::Signed))))
                .child(text("模拟当天签到尚未完成")),
        )
        .child(
            view()
                .on_press(move || {
                    sign_in_reply.set(Ok(SignInResult {
                        completion: Completion::AlreadySigned,
                        reward_claimed: false,
                        reward_points: None,
                    }))
                })
                .child(text("模拟签到已完成但奖励未确认")),
        )
        .child(
            view()
                .on_press(move || sign_in_reply.set(Ok(claimed(Completion::AlreadySigned))))
                .child(text("模拟签到状态已确认领取")),
        )
        .child(
            view()
                .on_press(move || sign_in_reply.set(Err(Failure::Timeout)))
                .child(text("模拟签到超时")),
        )
        .child(
            view()
                .on_press(move || sign_in_reply.set(Err(Failure::Verification)))
                .child(text("模拟签到需要人工验证")),
        )
        .child(
            view()
                .on_press(move || sign_in_reply.set(Err(Failure::Authorization)))
                .child(text("模拟签到授权失效")),
        )
        .child(text(move || {
            format!("签到请求次数：{}", sign_in_attempts.get())
        }))
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
                .on_press(move || qr_status.set(CONFIRMED_WITH_NEW_COOKIE))
                .child(text("模拟扫码确认新凭据")),
        )
        .child(
            view()
                .on_press(move || fail_authorization_save.set(true))
                .child(text("模拟下次授权写入失败")),
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
                                    community_cookie: None,
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
        .child(view().flex_row().flex_wrap().gap_2()
        .child(
            view()
                .flex_row()
                .gap_2()
                .child(
                    view()
                        .on_press(move || remove_environment.directories.borrow_mut().clear())
                        .child(text("模拟安装已移除")),
                )
                .child(
                    view()
                        .on_press(move || install_progress.set(40))
                        .child(text("模拟下载进度更新")),
                )
                .child(
                    view()
                        .on_press(move || {
                            interrupted_install.set(Some(DownloadOutcome::Interrupted))
                        })
                        .child(text("模拟下载网络中断")),
                )
                .child(
                    view()
                        .on_press(move || completed_install.set(Some(DownloadOutcome::Complete)))
                        .child(text("模拟下载完成")),
                ),
        )
        .child(
            view()
                .on_press(move || {
                    undiscoverable_environment.directories.replace(vec!["E:/手动目录".into()]);
                })
                .child(text("模拟未自动发现安装")),
        )
        .child(
            view()
                .on_press(move || {
                    single_environment.directories.replace(vec![
                        r"D:\Games\原神中文目录\GenshinImpactInstallationWithALongFolderName\Genshin Impact Game".into(),
                    ]);
                })
                .child(text("模拟仅发现一处安装")),
        )
        .child(
            view()
                .on_press(move || {
                    move_environment.directories.replace(vec!["F:/已移动/原神".into()]);
                })
                .child(text("模拟移动安装")),
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
        .child(
            view()
                .on_press(move || fail_plugin_catalog.set(true))
                .child(text("模拟插件目录加载失败")),
        )
        .child(
            view()
                .on_press(move || fail_plugin_catalog.set(false))
                .child(text("恢复插件目录加载")),
        )
        )
        .child(text(move || launched.get()))
        .child(dynamic(move || {
            view()
                .key(format!("session-{}", generation.get()))
                .flex_1()
                .child(launcher(environment.clone()))
        }))
}
