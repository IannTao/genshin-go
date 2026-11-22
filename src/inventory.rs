//! Locally imported, read-only inventory snapshots scoped to game UIDs.
mod archive;
pub(crate) mod backup;
mod mihoyo;
mod store;
mod ui;

use crate::{
    launcher::LauncherEnvironment,
    profile::{Profile, VerifiedAccount},
};
use nsh_core::prelude::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
};

const WEAPONS: u8 = 1;
const ARTIFACTS: u8 = 2;
const MATERIALS: u8 = 4;
const ALL: u8 = WEAPONS | ARTIFACTS | MATERIALS;

/// Defines the per-UID snapshot tables and the shared material catalog.
pub(crate) const SCHEMA: &str = store::SCHEMA;

/// Records the categories covered by existing and new inventory snapshots.
pub(crate) const COVERAGE_SCHEMA: &str =
    "ALTER TABLE inventory_snapshots ADD COLUMN coverage INTEGER NOT NULL DEFAULT 7
         CHECK (coverage BETWEEN 1 AND 7);";

/// Holds the collection time and quantities of a material inventory.
pub(crate) struct MaterialSnapshot {
    /// Contains the original collection time with its time zone.
    pub(crate) captured_at: String,
    /// Contains the quantities indexed by GOOD material key.
    pub(crate) quantities: BTreeMap<String, u32>,
}

/// Reads materials from the newest snapshot that contains them.
pub(crate) fn latest_materials(path: &Path, uid: &str) -> Result<Option<MaterialSnapshot>, String> {
    store::latest_materials(path, uid)
}

/// Fetches growth material counts for the verified account via `MiYouShe`.
pub(crate) async fn sync_mihoyo_materials(
    path: PathBuf,
    account: VerifiedAccount,
) -> Result<BTreeMap<String, u32>, String> {
    mihoyo::fetch_materials(path, account).await
}

/// Renders the warehouse and import controls for the selected UID.
pub(crate) fn section(
    environment: Rc<dyn LauncherEnvironment>,
    path: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    ui::section(environment, path, profile, revision)
}
