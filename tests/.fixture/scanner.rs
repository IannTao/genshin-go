use nsh_app_genshin_go::launcher::{
    LauncherEnvironment, ScannerInstance, ScannerLaunch, environment_in,
};
use nsh_core::prelude::*;
use std::{
    cell::{Cell, RefCell},
    fs,
    future::pending,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Hosts the local HTTP fixture and an isolated launcher database.
pub(super) struct Scanner {
    directory: PathBuf,
    pub(super) local: Box<dyn LauncherEnvironment>,
    port: u16,
    response: Arc<Mutex<Option<(u16, String)>>>,
    mode: Cell<Startup>,
    starts: Signal<u32>,
    closes: Signal<u32>,
}

#[derive(Clone, Copy)]
enum Startup {
    Accepted,
    Pending,
    Denied,
}

fn snapshot(session: &str, uid: &str, timestamp: u64) -> serde_json::Value {
    let mut body: serde_json::Value =
        serde_json::from_str(include_str!("scanner-response.json")).unwrap();
    body["sessionId"] = session.into();
    body["data"]["metadata"]["uid"] = uid.into();
    body["updatedAt"] = timestamp.into();
    body
}

impl Scanner {
    fn reply(&self, status: u16, body: String) {
        *self.response.lock().unwrap() = Some((status, body));
    }

    fn start(cold: bool, starts: Signal<u32>, closes: Signal<u32>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let directory = std::env::temp_dir().join(format!(
            "genshin-go-scanner-{}-{port}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("GGScanner.exe"), b"test placeholder").unwrap();
        let response = Arc::new(Mutex::new(Some((
            200,
            snapshot("previous-login", "111110042", 1795219200000).to_string(),
        ))));
        if !cold {
            Self::serve(listener, response.clone());
        }
        Self {
            local: Box::new(environment_in(&directory)),
            directory,
            port,
            response,
            mode: Cell::new(Startup::Accepted),
            starts,
            closes,
        }
    }

    /// Opens the simulated process without starting the manager until the test makes it ready.
    pub(super) fn launch(&self) -> ScannerLaunch {
        self.starts.update(|n| *n += 1);
        let mode = self.mode.get();
        let closes = self.closes;
        let response = self.response.clone();
        let port = self.port;
        Box::pin(async move {
            match mode {
                Startup::Accepted => Ok(ScannerInstance::new(move || {
                    closes.update(|n| *n += 1);
                    response.lock().unwrap().take();
                    wake_server(port);
                })),
                Startup::Pending => pending().await,
                Startup::Denied => Err("已取消 GGScanner 管理员权限请求".into()),
            }
        })
    }

    fn serve(listener: TcpListener, shared: Arc<Mutex<Option<(u16, String)>>>) {
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let Some((status, body)) = shared.lock().unwrap().clone() else {
                    break;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = String::new();
                let mut reader = BufReader::new(&stream);
                if reader.read_line(&mut request).is_err()
                    || !request.starts_with("GET /continuous HTTP/")
                {
                    continue;
                }
                for line in reader.lines().map_while(Result::ok) {
                    if line.is_empty() {
                        break;
                    }
                }
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        self.response.lock().unwrap().take();
        wake_server(self.port);
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn wake_server(port: u16) {
    let _ = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(100));
}

/// Renders controls for starting the service and publishing simulated game responses.
pub(super) fn controls(
    scanner: Rc<RefCell<Option<Scanner>>>,
    generation: Signal<u32>,
    launches: Signal<u32>,
    game_running: Signal<bool>,
) -> impl IntoElement {
    let address = signal(String::new());
    let program = signal(String::new());
    let source = signal(String::new());
    let prepared = signal(None::<String>);
    let publish = scanner.clone();
    let starts = signal(0_u32);
    let closes = signal(0_u32);
    view()
        .children(
            [("模拟扫描服务", false), ("模拟扫描器未开", true)].map(|(label, cold)| {
                let start = scanner.clone();
                view()
                    .on_press(move || {
                        let service = Scanner::start(cold, starts, closes);
                        address.set(format!("模拟端口：{}", service.port));
                        program.set(format!(
                            "模拟程序：{}",
                            service.directory.join("GGScanner.exe").display()
                        ));
                        start.replace(Some(service));
                        generation.update(|n| *n += 1);
                    })
                    .child(text(label))
            }),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .style_when(move || address.get().is_empty(), Styled::hidden)
                .children(
                    [
                        ("模拟等待提权", Startup::Pending),
                        ("模拟取消提权", Startup::Denied),
                    ]
                    .map(|(label, mode)| {
                        let scanner = scanner.clone();
                        view()
                            .on_press(move || {
                                if let Some(service) = scanner.borrow().as_ref() {
                                    service.mode.set(mode);
                                }
                            })
                            .child(text(label))
                    }),
                )
                .children(
                    [
                        ("模拟冷启动服务就绪", 503, "continuous_capture_no_data"),
                        ("模拟端口被占用", 404, "other_service"),
                    ]
                    .map(|(label, status, error)| {
                        let scanner = scanner.clone();
                        view()
                            .on_press(move || {
                                if let Some(service) = scanner.borrow().as_ref() {
                                    let listener =
                                        TcpListener::bind(("127.0.0.1", service.port)).unwrap();
                                    service.reply(status, format!(r#"{{"error":"{error}"}}"#));
                                    Scanner::serve(listener, service.response.clone());
                                }
                            })
                            .child(text(label))
                    }),
                )
                .child({
                    let scanner = scanner.clone();
                    view()
                        .on_press(move || {
                            if let Some(service) = scanner.borrow().as_ref() {
                                fs::remove_file(service.directory.join("GGScanner.exe")).unwrap();
                            }
                        })
                        .child(text("模拟扫描程序已移除"))
                })
                .child(
                    view()
                        .on_press(move || game_running.set(false))
                        .child(text("模拟游戏退出")),
                )
                .children(
                    [
                        ("模拟旧会话更新", "previous-login", "111110042", 2),
                        ("准备新登录快照", "current-login", "987650042", 4),
                        ("模拟角色尚未到达", "current-login", "987650042", 3),
                        ("模拟过期角色修订", "current-login", "987650042", 2),
                        ("模拟无效角色列表", "next-login", "987650042", 1),
                        ("模拟来源缺失", "missing-source", "999990042", 1),
                        ("模拟 UID 无效", "bad-uid", "123", 1),
                    ]
                    .into_iter()
                    .map(|(label, session, uid, revision)| {
                        let scanner = scanner.clone();
                        view()
                            .on_press(move || {
                                let timestamp = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis()
                                    .try_into()
                                    .unwrap();
                                let mut body = snapshot(session, uid, timestamp);
                                body["revision"] = revision.into();
                                if label == "模拟角色尚未到达" {
                                    body["data"].as_object_mut().unwrap().remove("characters");
                                }
                                if label == "模拟无效角色列表" {
                                    body["data"]["characters"][1]["talent"]["skill"] = (-1).into();
                                }
                                if label == "模拟来源缺失" {
                                    body["data"].as_object_mut().unwrap().remove("source");
                                }
                                if label == "准备新登录快照" {
                                    prepared.set(Some(body.to_string()));
                                } else if let Some(service) = scanner.borrow().as_ref() {
                                    service.reply(200, body.to_string());
                                }
                                source.set(format!("模拟来源毫秒：{timestamp}"));
                            })
                            .child(text(label))
                            .into_element()
                    }),
                )
                .children(
                    [
                        ("模拟服务禁用", 409, "continuous_capture_disabled"),
                        ("模拟采集失败", 503, "continuous_capture_failed"),
                        ("模拟等待登录", 503, "continuous_capture_no_data"),
                    ]
                    .into_iter()
                    .map(|(label, status, error)| {
                        let scanner = scanner.clone();
                        view()
                            .on_press(move || {
                                let body = format!(r#"{{"error":"{error}"}}"#);
                                if let Some(service) = scanner.borrow().as_ref() {
                                    service.reply(status, body);
                                }
                            })
                            .child(text(label))
                            .into_element()
                    }),
                )
                .child(
                    view()
                        .on_press(move || {
                            if let (Some(service), Some(body)) =
                                (publish.borrow().as_ref(), prepared.get_untracked())
                            {
                                service.reply(200, body);
                            }
                        })
                        .child(text("发布新登录快照")),
                )
                .child(text(move || program.get()))
                .child(text(move || address.get()))
                .child(text(move || source.get()))
                .child(text(move || format!("扫描器启动请求：{}", starts.get())))
                .child(text(move || format!("扫描器关闭请求：{}", closes.get())))
                .child(text(move || format!("游戏启动次数：{}", launches.get()))),
        )
}
