//! UIGF archive handling, storage and launcher controls for wish records.
mod archive;
mod store;
mod ui;

/// Creates the UID-owned wish table and its browsing index.
pub(crate) const SCHEMA: &str = "
    CREATE TABLE wish_records (
        uid TEXT NOT NULL REFERENCES uid_history(uid) ON DELETE CASCADE,
        record_id TEXT NOT NULL,
        drawn_at TEXT NOT NULL,
        record_json TEXT NOT NULL,
        PRIMARY KEY (uid, record_id)
    );
    CREATE INDEX wish_records_by_time
        ON wish_records (uid, drawn_at DESC, length(record_id) DESC, record_id DESC);
";

/// Renders wish import, browsing and export for the Windows launcher.
pub(crate) fn section(
    path: Result<std::path::PathBuf, String>,
    profile: nsh_core::prelude::Signal<crate::profile::Profile>,
    revision: nsh_core::prelude::Signal<u64>,
) -> nsh_core::prelude::View {
    ui::section(path, profile, revision)
}
