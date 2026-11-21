//! Locally imported, read-only inventory snapshots scoped to game UIDs.
mod archive;
mod store;
mod ui;

use crate::profile::Profile;
use nsh_core::prelude::*;
use std::path::PathBuf;

/// Defines the per-UID snapshot tables and the shared material catalog.
pub(crate) const SCHEMA: &str = store::SCHEMA;

/// Renders the inventory warehouse within the UID data spaces page.
pub(crate) fn section(
    path: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    ui::section(path, profile, revision)
}
