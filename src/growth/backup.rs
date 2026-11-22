//! Portable growth plans and manual stock for one game UID.

use super::{Catalog, Kind, MaterialCounts, Plan, Progress, requirements, store};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Holds both kinds of saved plans and the UID's shared manual stock.
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Data {
    characters: BTreeMap<String, Plan>,
    weapons: BTreeMap<String, Plan>,
    stock: MaterialCounts,
}

impl Data {
    /// Captures the UID's planning data from the caller's database snapshot.
    pub(crate) fn capture(db: &Connection, uid: &str) -> Result<Self, String> {
        Ok(Self {
            characters: store::plans(db, uid, Kind::Character)?,
            weapons: store::plans(db, uid, Kind::Weapon)?,
            stock: store::stock(db, uid)?,
        })
    }

    /// Checks that the plans can be opened by the planners.
    pub(crate) fn validate(&self) -> Result<(), String> {
        for (kind, plans) in [
            (Kind::Character, &self.characters),
            (Kind::Weapon, &self.weapons),
        ] {
            if plans.is_empty() {
                continue;
            }
            let catalog = Catalog::load(kind)?;
            for (key, plan) in plans {
                let item = catalog
                    .items
                    .iter()
                    .find(|item| &item.key == key)
                    .ok_or_else(|| format!("未知{}养成计划：{key}", kind.subject()))?;
                requirements(catalog, item, *plan)
                    .map_err(|error| format!("{}的养成计划无效：{error}", item.name))?;
            }
        }
        if self.stock.keys().any(|&id| id <= 0) {
            return Err("手填库存包含无效材料 ID".into());
        }
        Ok(())
    }

    /// Reports whether the backup carries planning data for this UID.
    pub(crate) fn is_empty(&self) -> bool {
        self.characters.is_empty() && self.weapons.is_empty() && self.stock.is_empty()
    }

    /// Compares the incoming values with the corresponding local values.
    pub(crate) fn plan(&self, before: &Self) -> Restore {
        Restore {
            characters: Changes::new(&self.characters, &before.characters),
            weapons: Changes::new(&self.weapons, &before.weapons),
            stock: Changes::new(&self.stock, &before.stock),
        }
    }
}

/// Identifies the user-selected version of a conflicting value.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Version {
    /// Keeps the locally saved value.
    Local,
    /// Restores the value in the backup.
    Backup,
}

/// Identifies a conflicting plan or manual quantity within one UID.
#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Target {
    /// Identifies a character by its catalog key.
    Character(String),
    /// Identifies a weapon by its catalog key.
    Weapon(String),
    /// Identifies a manual quantity by material ID.
    Stock(i32),
}

/// Holds the versions selected for one UID's conflicts.
pub(crate) type Choices = BTreeMap<Target, Version>;

#[derive(Eq, PartialEq)]
struct Conflict<K, V> {
    key: K,
    local: V,
    backup: V,
}

#[derive(Eq, PartialEq)]
struct Changes<K: Ord, V> {
    additions: BTreeMap<K, V>,
    duplicates: usize,
    conflicts: Vec<Conflict<K, V>>,
}

impl<K: Ord + Clone, V: Copy + Eq> Changes<K, V> {
    fn new(incoming: &BTreeMap<K, V>, before: &BTreeMap<K, V>) -> Self {
        let mut changes = Self {
            additions: BTreeMap::new(),
            duplicates: 0,
            conflicts: Vec::new(),
        };
        for (key, &backup) in incoming {
            match before.get(key) {
                None => {
                    changes.additions.insert(key.clone(), backup);
                }
                Some(&local) if local == backup => changes.duplicates += 1,
                Some(&local) => changes.conflicts.push(Conflict {
                    key: key.clone(),
                    local,
                    backup,
                }),
            }
        }
        changes
    }

    fn summary(&self, uid: &str, category: &str) -> String {
        format!(
            "UID {uid} · {category}新增 {}，重复 {}，冲突 {}",
            self.additions.len(),
            self.duplicates,
            self.conflicts.len()
        )
    }

    fn writes<'a>(
        &'a self,
        use_backup: impl Fn(&K) -> bool + 'a,
    ) -> impl Iterator<Item = (&'a K, &'a V)> {
        self.additions
            .iter()
            .chain(self.conflicts.iter().filter_map(move |conflict| {
                use_backup(&conflict.key).then_some((&conflict.key, &conflict.backup))
            }))
    }
}

/// Describes the additions, duplicates and conflicts of one UID's planning data.
#[derive(Eq, PartialEq)]
pub(crate) struct Restore {
    characters: Changes<String, Plan>,
    weapons: Changes<String, Plan>,
    stock: Changes<i32, u32>,
}

impl Restore {
    /// Reports whether the restore adds or replaces planning data.
    pub(crate) fn has_changes(&self, choices: Option<&Choices>) -> bool {
        !self.characters.additions.is_empty()
            || !self.weapons.additions.is_empty()
            || !self.stock.additions.is_empty()
            || self.targets().any(|target| uses_backup(choices, &target))
    }

    /// Checks whether every conflict has an explicit choice.
    pub(crate) fn choices_complete(&self, choices: Option<&Choices>) -> bool {
        self.targets()
            .all(|target| choices.is_some_and(|choices| choices.contains_key(&target)))
    }

    fn targets(&self) -> impl Iterator<Item = Target> + '_ {
        self.characters
            .conflicts
            .iter()
            .map(|conflict| Target::Character(conflict.key.clone()))
            .chain(
                self.weapons
                    .conflicts
                    .iter()
                    .map(|conflict| Target::Weapon(conflict.key.clone())),
            )
            .chain(
                self.stock
                    .conflicts
                    .iter()
                    .map(|conflict| Target::Stock(conflict.key)),
            )
    }

    /// Summarizes the changes for each kind of plan and the manual stock.
    pub(crate) fn summaries(&self, uid: &str) -> [String; 3] {
        [
            self.characters.summary(uid, "角色计划"),
            self.weapons.summary(uid, "武器计划"),
            self.stock.summary(uid, "手填库存"),
        ]
    }

    /// Lists the key, subject, local value and backup value for each conflict.
    pub(crate) fn conflicts(&self) -> impl Iterator<Item = (Target, String, String, String)> {
        [
            (Kind::Character, &self.characters),
            (Kind::Weapon, &self.weapons),
        ]
        .into_iter()
        .flat_map(|(kind, plans)| {
            let catalog = Catalog::load(kind).ok();
            plans.conflicts.iter().map(move |conflict| {
                let name = catalog
                    .and_then(|catalog| catalog.items.iter().find(|item| item.key == conflict.key))
                    .map_or(conflict.key.as_str(), |item| item.name.as_str());
                (
                    plan_target(kind, &conflict.key),
                    format!("{}计划 {name}", kind.subject()),
                    describe(conflict.local),
                    describe(conflict.backup),
                )
            })
        })
        .chain(self.stock.conflicts.iter().map(|conflict| {
            (
                Target::Stock(conflict.key),
                stock_label(conflict.key),
                format!("数量 {}", conflict.local),
                format!("数量 {}", conflict.backup),
            )
        }))
    }

    /// Writes additions and the backup versions chosen for individual records.
    pub(crate) fn apply(
        &self,
        db: &Connection,
        uid: &str,
        choices: Option<&Choices>,
    ) -> Result<(), String> {
        for (kind, plans) in [
            (Kind::Character, &self.characters),
            (Kind::Weapon, &self.weapons),
        ] {
            for (key, &plan) in plans.writes(|key| uses_backup(choices, &plan_target(kind, key))) {
                store::save_plan(db, uid, kind, key, plan)?;
            }
        }
        store::save_stock(
            db,
            uid,
            self.stock
                .writes(|&id| uses_backup(choices, &Target::Stock(id)))
                .map(|(&id, &amount)| (id, amount)),
        )
    }
}

fn uses_backup(choices: Option<&Choices>, target: &Target) -> bool {
    choices.and_then(|choices| choices.get(target)) == Some(&Version::Backup)
}

fn plan_target(kind: Kind, key: &str) -> Target {
    match kind {
        Kind::Character => Target::Character(key.to_owned()),
        Kind::Weapon => Target::Weapon(key.to_owned()),
    }
}

fn stock_label(id: i32) -> String {
    for kind in [Kind::Character, Kind::Weapon] {
        if let Some((_, name, _)) = kind
            .experience_materials()
            .iter()
            .find(|entry| entry.0 == id)
        {
            return format!("手填库存 {name}（材料 {id}）");
        }
        if let Ok(catalog) = Catalog::load(kind) {
            let name = catalog.material_name(id);
            if name != "未知材料" {
                return format!("手填库存 {name}（材料 {id}）");
            }
        }
    }
    format!("手填库存 材料 {id}")
}

fn describe(plan: Plan) -> String {
    fn progress(state: Progress) -> String {
        let mut value = format!("等级 {}、突破 {}", state.level, state.phase);
        if let Some([normal, skill, burst]) = state.talents {
            use std::fmt::Write as _;
            let _ = write!(value, "、天赋 {normal}/{skill}/{burst}");
        }
        value
    }
    format!("{} → {}", progress(plan.current), progress(plan.target))
}
