//! Locally imported, read-only inventory snapshots scoped to game UIDs.
mod archive;
mod store;
mod ui;

use crate::profile::Profile;
use nsh_core::prelude::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Defines the per-UID snapshot tables and the shared material catalog.
pub(crate) const SCHEMA: &str = store::SCHEMA;

/// Holds the collection time and material quantities of a complete inventory snapshot.
pub(crate) struct MaterialSnapshot {
    /// Contains the original collection time with its time zone.
    pub(crate) captured_at: String,
    /// Contains the quantities indexed by GOOD material key.
    pub(crate) quantities: BTreeMap<String, u32>,
}

/// Reads materials from the latest complete snapshot for the given game UID.
pub(crate) fn latest_materials(path: &Path, uid: &str) -> Result<Option<MaterialSnapshot>, String> {
    store::latest_materials(path, uid)
}

/// Renders the inventory warehouse within the UID data spaces page.
pub(crate) fn section(
    path: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    ui::section(path, profile, revision)
}
