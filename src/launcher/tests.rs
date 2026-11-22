use super::{
    DirectoryChoice, LauncherEnvironment, SIDEBAR_COLLAPSED_WIDTH, SIDEBAR_EXPANDED_WIDTH, launcher,
};
use crate::{
    accounts::{AccountOperation, Authorization, QrSession, QrStatus},
    profile::{Profile, VerifiedAccount},
};
use nsh_core::{
    ApproxTextEngine, Bounds, Core, EventKind, EventPayload, MountBatch, MountItem, NodeId, Points,
    element::Root, prelude::*,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};

struct LayoutEnvironment {
    discovered: Vec<PathBuf>,
    saved: Option<PathBuf>,
    uids: Vec<String>,
    #[cfg(target_os = "windows")]
    database: Option<PathBuf>,
}

impl LauncherEnvironment for LayoutEnvironment {
    #[cfg(target_os = "windows")]
    fn uid_database_path(&self) -> Option<Result<PathBuf, String>> {
        self.database.clone().map(Ok)
    }

    fn check_authorization(&self, _account_id: String) -> AccountOperation<bool> {
        Box::pin(async { Ok(false) })
    }

    fn create_qr(&self) -> AccountOperation<QrSession> {
        Box::pin(async { Err("unused".into()) })
    }

    fn poll_qr(&self, _qr: QrSession) -> AccountOperation<QrStatus> {
        Box::pin(async { Ok(QrStatus::Waiting) })
    }

    fn load_profile(&self) -> Result<Profile, String> {
        Ok(Profile {
            active_uid: self.uids.first().cloned(),
            uids: self.uids.clone(),
            account: None,
        })
    }

    fn select_uid(&self, _uid: &str, _create: bool) -> Result<Profile, String> {
        self.load_profile()
    }

    fn delete_uid_history(&self, _uid: &str) -> Result<Profile, String> {
        self.load_profile()
    }

    fn verify_uid(&self, _authorization: Authorization) -> AccountOperation<String> {
        Box::pin(async { Err("unused".into()) })
    }

    fn save_authorization(
        &self,
        _authorization: Authorization,
        _uid: &str,
        _expected: Option<&VerifiedAccount>,
    ) -> Result<Profile, String> {
        self.load_profile()
    }

    fn remove_authorization(&self, _account: &VerifiedAccount) -> Result<Profile, String> {
        self.load_profile()
    }

    fn discover(&self) -> Result<Vec<PathBuf>, String> {
        Ok(self.discovered.clone())
    }

    fn load_default(&self) -> Result<Option<PathBuf>, String> {
        Ok(self.saved.clone())
    }

    fn save_default(&self, _directory: &Path) -> Result<(), String> {
        Ok(())
    }

    fn validate(&self, directory: &Path) -> Result<PathBuf, String> {
        Ok(directory.to_path_buf())
    }

    fn launch(&self, _directory: &Path) -> Result<(), String> {
        Ok(())
    }

    fn choose_directory(&self) -> DirectoryChoice {
        Box::pin(async { Ok(None) })
    }
}

#[derive(Clone, PartialEq, Props)]
struct TestLauncherProps {
    discovered: Prop<Vec<String>>,
    saved: Prop<Option<String>>,
    uids: Prop<Vec<String>>,
    database: Prop<Option<String>>,
}

#[component]
fn TestLauncher(props: TestLauncherProps) -> impl IntoElement {
    launcher(Rc::new(LayoutEnvironment {
        discovered: props
            .discovered
            .get_untracked()
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        saved: props.saved.get_untracked().map(PathBuf::from),
        uids: props.uids.get_untracked(),
        #[cfg(target_os = "windows")]
        database: props.database.get_untracked().map(PathBuf::from),
    }))
}

struct LayoutHarness {
    core: Core<ApproxTextEngine>,
    ids: HashMap<String, NodeId>,
    frames: HashMap<NodeId, Bounds<Points>>,
}

impl LayoutHarness {
    fn open(
        width: f32,
        height: f32,
        discovered: Vec<PathBuf>,
        saved: Option<PathBuf>,
        uids: Vec<String>,
    ) -> Self {
        Self::open_with_database(width, height, discovered, saved, uids, None)
    }

    fn open_with_database(
        width: f32,
        height: f32,
        discovered: Vec<PathBuf>,
        saved: Option<PathBuf>,
        uids: Vec<String>,
        database: Option<PathBuf>,
    ) -> Self {
        let discovered: Vec<String> = discovered
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let saved = saved.map(|path| path.to_string_lossy().into_owned());
        let database = database.map(|path| path.to_string_lossy().into_owned());
        let mut core = Core::new(ApproxTextEngine, size(px(width), px(height)));
        core.mount_root(&Root::new::<TestLauncher>(move || TestLauncherProps {
            discovered: Prop::new(discovered.clone()),
            saved: Prop::new(saved.clone()),
            uids: Prop::new(uids.clone()),
            database: Prop::new(database.clone()),
        }));
        let mut harness = Self {
            core,
            ids: HashMap::new(),
            frames: HashMap::new(),
        };
        harness.sync();
        harness
    }

    fn sync(&mut self) {
        if let Some(batch) = self.core.commit() {
            self.record(batch);
        }
    }

    fn record(&mut self, batch: MountBatch) {
        for item in batch.items {
            match item {
                MountItem::ViewProps { id, props } => {
                    if let Some(test_id) = props.test_id {
                        self.ids.insert(test_id.to_string(), id);
                    }
                }
                MountItem::Frame { id, frame } => {
                    self.frames.insert(id, frame);
                }
                MountItem::Delete { id } => {
                    self.frames.remove(&id);
                    self.ids.retain(|_, value| *value != id);
                }
                _ => {}
            }
        }
    }

    fn press(&mut self, test_id: &str) {
        let Some(&id) = self.ids.get(test_id) else {
            panic!("missing node for test_id {test_id}");
        };
        assert!(
            self.core
                .component_event(id, EventKind::Press, &EventPayload::None),
            "node {test_id} did not handle press"
        );
        self.sync();
    }

    fn set_viewport(&mut self, width: f32, height: f32) {
        self.core.set_viewport(size(px(width), px(height)));
        self.sync();
    }

    fn frame(&self, test_id: &str) -> Bounds<Points> {
        let Some(&id) = self.ids.get(test_id) else {
            panic!("missing node for test_id {test_id}");
        };
        let Some(&frame) = self.frames.get(&id) else {
            panic!("missing frame for test_id {test_id}");
        };
        frame
    }
}

#[test]
fn sidebar_toggles_between_212_and_72_px_and_widens_content() {
    let mut app = LayoutHarness::open(
        1040.0,
        720.0,
        Vec::new(),
        Some(PathBuf::from(r"D:\Genshin")),
        Vec::new(),
    );

    let expanded_sidebar = app.frame("launcher-sidebar");
    let expanded_content = app.frame("launcher-content");
    let expanded_settings = app.frame("launcher-sidebar-bottom");
    let expanded_home = app.frame("nav-home");

    assert!((expanded_sidebar.size.width.0 - SIDEBAR_EXPANDED_WIDTH).abs() < 0.5);
    assert!((expanded_content.size.width.0 - (1040.0 - SIDEBAR_EXPANDED_WIDTH - 1.0)).abs() < 0.5);
    assert!(expanded_settings.origin.y.0 > expanded_home.origin.y.0);

    app.press("launcher-sidebar-toggle");

    let collapsed_sidebar = app.frame("launcher-sidebar");
    let collapsed_content = app.frame("launcher-content");
    let collapsed_settings = app.frame("launcher-sidebar-bottom");

    assert!((collapsed_sidebar.size.width.0 - SIDEBAR_COLLAPSED_WIDTH).abs() < 0.5);
    assert!(
        (collapsed_content.size.width.0 - (1040.0 - SIDEBAR_COLLAPSED_WIDTH - 1.0)).abs() < 0.5
    );
    assert!(
        (collapsed_content.size.width.0
            - expanded_content.size.width.0
            - (SIDEBAR_EXPANDED_WIDTH - SIDEBAR_COLLAPSED_WIDTH))
            .abs()
            < 0.5
    );
    assert!(collapsed_settings.size.height.0 > 0.0);
    assert!(collapsed_settings.origin.y.0 > 500.0);
}

#[test]
fn narrow_window_crops_home_landscape_first_and_keeps_ordinary_launch_usable() {
    let mut app = LayoutHarness::open(
        1040.0,
        720.0,
        Vec::new(),
        Some(PathBuf::from(r"D:\Genshin")),
        Vec::new(),
    );
    let wide_copy = app.frame("launcher-home-copy");
    let wide_landscape = app.frame("launcher-home-landscape");
    assert!(wide_landscape.size.width.0 > 400.0);
    assert!(wide_copy.size.width.0 >= 236.0);

    app.set_viewport(520.0, 640.0);
    let narrow_copy = app.frame("launcher-home-copy");
    let narrow_landscape = app.frame("launcher-home-landscape");
    let launch_button = app.frame("launcher-ordinary-launch");
    assert!(narrow_landscape.size.width.0 < 80.0);
    assert!(narrow_copy.size.width.0 >= 236.0);
    assert!(launch_button.size.width.0 > 0.0);
    assert!(launch_button.size.height.0 > 0.0);
    assert!(
        launch_button.origin.x.0 + launch_button.size.width.0 <= narrow_copy.size.width.0 + 1.0
    );

    app.set_viewport(420.0, 640.0);
    let tight_copy = app.frame("launcher-home-copy");
    let tight_landscape = app.frame("launcher-home-landscape");
    let tight_launch = app.frame("launcher-ordinary-launch");
    assert!(tight_landscape.size.width.0 <= 1.0);
    assert!(tight_copy.size.width.0 > 150.0);
    assert!(tight_launch.size.width.0 > 0.0);
    assert!(tight_launch.origin.x.0 + tight_launch.size.width.0 <= tight_copy.size.width.0 + 1.0);
}

#[test]
fn narrow_window_wraps_long_directory_path_without_horizontal_overflow() {
    let long_path = PathBuf::from(
        r"D:\Games\原神中文目录\GenshinImpactInstallationWithALongFolderName\Genshin Impact Game",
    );
    let app = LayoutHarness::open(520.0, 640.0, vec![long_path], None, Vec::new());

    let copy = app.frame("launcher-home-copy");
    let path_frame = app.frame("launcher-home-candidate-path");

    assert!(path_frame.size.width.0 > 0.0);
    assert!(path_frame.size.width.0 <= copy.size.width.0 + 1.0);
    assert!(path_frame.size.height.0 > 28.0);
}

#[cfg(target_os = "windows")]
#[test]
fn data_pages_reflow_controls_when_window_narrows() -> Result<(), String> {
    let temp = std::env::temp_dir().join(format!(
        "genshin-go-layout-{}-{}.sqlite3",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    crate::account_store::select_uid(&temp, "100000001", true)?;
    let mut app = LayoutHarness::open_with_database(
        1040.0,
        720.0,
        Vec::new(),
        None,
        vec!["100000001".to_owned()],
        Some(temp.clone()),
    );
    app.press("nav-achievements");

    let wide_row = app.frame("achievement-row-80001");
    let wide_content = app.frame("launcher-content");
    assert!(wide_row.size.width.0 > 0.0);
    assert!(wide_row.size.width.0 <= wide_content.size.width.0 + 1.0);

    app.set_viewport(480.0, 640.0);
    let narrow_row = app.frame("achievement-row-80001");
    let narrow_content = app.frame("launcher-content");
    assert!(narrow_row.size.width.0 <= narrow_content.size.width.0 + 1.0);
    assert!(
        narrow_row.size.height.0 > wide_row.size.height.0,
        "expected achievement row to wrap at narrow width (wide={}, narrow={})",
        wide_row.size.height.0,
        narrow_row.size.height.0
    );

    let _ = std::fs::remove_file(&temp);
    let _ = std::fs::remove_file(temp.with_extension("sqlite3-wal"));
    let _ = std::fs::remove_file(temp.with_extension("sqlite3-shm"));
    Ok(())
}
