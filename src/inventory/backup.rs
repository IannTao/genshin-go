//! Portable history of inventory snapshots for one game UID.

use super::{
    archive::{CaptureTime, Inventory, valid_good_source},
    store,
};
use crate::archive_json::Object;
use rusqlite::Connection;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::{BTreeMap, btree_map::Entry};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    captured_at: String,
    source: String,
    good_version: u8,
    #[serde(default = "complete", skip_serializing_if = "is_complete")]
    coverage: u8,
    inventory: Object<Inventory>,
}

const fn complete() -> u8 {
    super::ALL
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "Serde passes the field by reference"
)]
fn is_complete(coverage: &u8) -> bool {
    *coverage == complete()
}

/// Holds the inventory snapshot history keyed by the instant each snapshot was collected.
#[derive(Default, Deserialize)]
#[serde(try_from = "Vec<Object<Snapshot>>")]
pub(crate) struct Data(BTreeMap<i64, Snapshot>);

impl Serialize for Data {
    fn serialize<S: Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        encoder.collect_seq(self.0.values().rev())
    }
}

impl TryFrom<Vec<Object<Snapshot>>> for Data {
    type Error = String;

    fn try_from(snapshots: Vec<Object<Snapshot>>) -> Result<Self, String> {
        let mut history = BTreeMap::new();
        for Object(mut snapshot) in snapshots {
            let time = CaptureTime::parse(&snapshot.captured_at)?;
            if !valid_good_source(snapshot.good_version, &snapshot.source) {
                return Err("背包快照须包含有效的 GOOD 版本和来源".into());
            }
            snapshot.inventory.0.validate_coverage(snapshot.coverage)?;
            snapshot.captured_at = time.original;
            match history.entry(time.nanos) {
                Entry::Vacant(entry) => {
                    entry.insert(snapshot);
                }
                Entry::Occupied(entry) => {
                    return Err(format!(
                        "整体备份包含同一采集时间的多个背包快照：{}",
                        entry.get().captured_at
                    ));
                }
            }
        }
        Ok(Self(history))
    }
}

impl Data {
    /// Captures all inventory snapshots from the caller's database snapshot.
    pub(crate) fn capture(db: &Connection, uid: &str) -> Result<Self, String> {
        store::headers(db, uid)?
            .into_iter()
            .map(|header| {
                Ok((
                    header.captured_nanos,
                    Snapshot {
                        captured_at: header.captured_at,
                        source: header.source,
                        good_version: header.version,
                        coverage: header.coverage,
                        inventory: Object(store::inventory(db, uid, header.id)?),
                    },
                ))
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }

    /// Reports whether the backup carries any snapshots for this UID.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Checks that each restored snapshot can be backed up by the GOOD importer.
    pub(crate) fn validate(&self, uid: &str) -> Result<(), String> {
        for snapshot in self.0.values() {
            snapshot.inventory.0.encode(
                snapshot.good_version,
                &snapshot.source,
                uid,
                &snapshot.captured_at,
                snapshot.coverage,
            )?;
        }
        Ok(())
    }

    /// Plans a union by collection time, keeping local contents when the same instant conflicts.
    pub(crate) fn plan(&self, before: &Self) -> Restore {
        let mut restore = Restore::default();
        for (&instant, snapshot) in &self.0 {
            match before.0.get(&instant) {
                None => restore.additions.push(instant),
                Some(local)
                    if local.coverage == snapshot.coverage
                        && local.inventory.0 == snapshot.inventory.0 =>
                {
                    restore.duplicates += 1;
                }
                Some(_) => restore.conflicts.push(snapshot.captured_at.clone()),
            }
        }
        let latest = match (before.0.last_key_value(), self.0.last_key_value()) {
            (local, Some(incoming)) if local.is_none_or(|(instant, _)| instant < incoming.0) => {
                Some(incoming)
            }
            (local, _) => local,
        };
        restore.latest = latest.map(|(_, snapshot)| snapshot.captured_at.clone());
        restore
    }

    /// Writes each reviewed addition as a whole snapshot in the caller's transaction.
    pub(crate) fn apply(
        &self,
        db: &Connection,
        uid: &str,
        restore: &Restore,
    ) -> Result<(), String> {
        for instant in &restore.additions {
            let snapshot = &self.0[instant];
            let time = CaptureTime {
                original: snapshot.captured_at.clone(),
                nanos: *instant,
            };
            store::insert(
                db,
                uid,
                &time,
                snapshot.good_version,
                &snapshot.source,
                &snapshot.inventory.0,
                snapshot.coverage,
            )?;
        }
        Ok(())
    }
}

/// Describes the observable result of merging the inventory histories.
#[derive(Default, Eq, PartialEq)]
pub(crate) struct Restore {
    additions: Vec<i64>,
    duplicates: usize,
    conflicts: Vec<String>,
    latest: Option<String>,
}

impl Restore {
    /// Reports whether the restore adds any snapshots.
    pub(crate) fn has_additions(&self) -> bool {
        !self.additions.is_empty()
    }

    /// Summarizes the history merge and the resulting default collection time.
    pub(crate) fn summaries(&self, uid: &str) -> impl Iterator<Item = String> + '_ {
        let summary = format!(
            "UID {uid} · 背包快照新增 {}，重复 {}，冲突 {}",
            self.additions.len(),
            self.duplicates,
            self.conflicts.len()
        );
        let latest = self
            .latest
            .as_ref()
            .map(|time| format!("恢复后默认采集时间：{time}"));
        let conflicts = (!self.conflicts.is_empty()).then(|| {
            format!(
                "同一采集时间的背包冲突保留本机快照：{}",
                self.conflicts.join("、")
            )
        });
        std::iter::once(summary).chain(latest).chain(conflicts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn partial_json(coverage: u8, materials: &serde_json::Value) -> String {
        serde_json::json!([{
            "captured_at": "2026-11-01T08:00:00+08:00",
            "source": "yas-GOODScanner",
            "good_version": 3,
            "coverage": coverage,
            "inventory": {
                "weapons": [],
                "artifacts": [],
                "materials": materials
            }
        }])
        .to_string()
    }

    #[test]
    fn validates_partial_snapshot_coverage_and_detects_coverage_conflicts()
    -> Result<(), Box<dyn std::error::Error>> {
        let inconsistent = partial_json(super::super::WEAPONS, &serde_json::json!({"Mora": 1}));
        assert!(serde_json::from_str::<Data>(&inconsistent).is_err());

        let equipment: Data =
            serde_json::from_str(&partial_json(super::super::WEAPONS, &serde_json::json!({})))?;
        let encoded = serde_json::to_string(&equipment)?;
        assert!(encoded.contains("\"coverage\":1"));
        let roundtrip: Data = serde_json::from_str(&encoded)?;
        assert_eq!(equipment.plan(&roundtrip).duplicates, 1);

        let complete: Data =
            serde_json::from_str(&partial_json(super::super::ALL, &serde_json::json!({})))?;
        let restore = complete.plan(&equipment);
        assert_eq!(restore.conflicts, vec!["2026-11-01T08:00:00+08:00"]);
        assert!(!restore.has_additions());
        Ok(())
    }
}
