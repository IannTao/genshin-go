use super::{Kind, MaterialCounts, Plan};
use crate::account_store::{database_error, open};
use rusqlite::{Connection, params};
use std::{collections::BTreeMap, path::Path};

/// Creates the per-UID character plans and shared manual stock tables.
pub(super) const SCHEMA: &str = "
    CREATE TABLE character_growth_plans (
        uid TEXT NOT NULL REFERENCES uid_history(uid) ON DELETE CASCADE,
        character_key TEXT NOT NULL,
        plan_json TEXT NOT NULL,
        PRIMARY KEY (uid, character_key)
    );
    CREATE TABLE growth_material_stock (
        uid TEXT NOT NULL REFERENCES uid_history(uid) ON DELETE CASCADE,
        material_id INTEGER NOT NULL,
        quantity INTEGER NOT NULL CHECK (quantity >= 0),
        PRIMARY KEY (uid, material_id)
    );";

/// Creates the weapon plans table under the UID history deletion boundary.
pub(super) const WEAPON_SCHEMA: &str = "
    CREATE TABLE weapon_growth_plans (
        uid TEXT NOT NULL REFERENCES uid_history(uid) ON DELETE CASCADE,
        weapon_key TEXT NOT NULL,
        plan_json TEXT NOT NULL,
        PRIMARY KEY (uid, weapon_key)
    );";

/// Holds the saved plans and manual quantities for one UID.
pub(super) struct Saved {
    /// Contains the plans of the requested kind.
    pub(super) plans: BTreeMap<String, Plan>,
    /// Contains the manual material quantities shared by both planners.
    pub(super) stock: MaterialCounts,
}

/// Reads the saved planning data from one snapshot of the UID's history.
pub(super) fn load(path: &Path, uid: &str, kind: Kind) -> Result<Saved, String> {
    let mut db = open(path)?;
    let read = db.transaction().map_err(|error| database_error(&error))?;
    Ok(Saved {
        plans: plans(&read, uid, kind)?,
        stock: stock(&read, uid)?,
    })
}

/// Reads the specified kind of growth plan from the caller's database snapshot.
pub(super) fn plans(
    db: &Connection,
    uid: &str,
    kind: Kind,
) -> Result<BTreeMap<String, Plan>, String> {
    let sql = match kind {
        Kind::Character => {
            "SELECT character_key, plan_json FROM character_growth_plans WHERE uid = ?1"
        }
        Kind::Weapon => "SELECT weapon_key, plan_json FROM weapon_growth_plans WHERE uid = ?1",
    };
    let mut plans = db.prepare(sql).map_err(|error| database_error(&error))?;
    plans
        .query_map([uid], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| database_error(&error))?
        .map(|row| {
            let (key, json) = row.map_err(|error| database_error(&error))?;
            let plan = serde_json::from_str(&json)
                .map_err(|error| format!("无法读取养成计划：{error}"))?;
            Ok((key, plan))
        })
        .collect()
}

/// Reads the shared manual material quantities from the caller's database snapshot.
pub(super) fn stock(db: &Connection, uid: &str) -> Result<MaterialCounts, String> {
    let mut stock = db
        .prepare("SELECT material_id, quantity FROM growth_material_stock WHERE uid = ?1")
        .map_err(|error| database_error(&error))?;
    stock
        .query_map([uid], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|error| database_error(&error))?
        .collect::<Result<_, _>>()
        .map_err(|error| database_error(&error))
}

/// Saves the plan and updates only the quantities edited with it, atomically.
pub(super) fn save(
    path: &Path,
    uid: &str,
    kind: Kind,
    key: &str,
    plan: Plan,
    stock: &MaterialCounts,
) -> Result<(), String> {
    let mut db = open(path)?;
    let write = db.transaction().map_err(|error| database_error(&error))?;
    save_plan(&write, uid, kind, key, plan)?;
    save_stock(&write, uid, stock.iter().map(|(&id, &amount)| (id, amount)))?;
    write.commit().map_err(|error| database_error(&error))
}

/// Writes one growth plan inside the caller's transaction.
pub(super) fn save_plan(
    db: &Connection,
    uid: &str,
    kind: Kind,
    key: &str,
    plan: Plan,
) -> Result<(), String> {
    let json =
        serde_json::to_string(&plan).map_err(|error| format!("无法编码养成计划：{error}"))?;
    let sql = match kind {
        Kind::Character => {
            "INSERT INTO character_growth_plans VALUES (?1, ?2, ?3)
            ON CONFLICT(uid, character_key) DO UPDATE SET plan_json = excluded.plan_json"
        }
        Kind::Weapon => {
            "INSERT INTO weapon_growth_plans VALUES (?1, ?2, ?3)
            ON CONFLICT(uid, weapon_key) DO UPDATE SET plan_json = excluded.plan_json"
        }
    };
    db.execute(sql, params![uid, key, json])
        .map_err(|error| database_error(&error))?;
    Ok(())
}

/// Upserts the supplied manual quantities inside the caller's transaction.
pub(super) fn save_stock(
    db: &Connection,
    uid: &str,
    stock: impl IntoIterator<Item = (i32, u32)>,
) -> Result<(), String> {
    let mut upsert = db
        .prepare(
            "INSERT INTO growth_material_stock VALUES (?1, ?2, ?3)
             ON CONFLICT(uid, material_id) DO UPDATE SET quantity = excluded.quantity",
        )
        .map_err(|error| database_error(&error))?;
    for (id, amount) in stock {
        upsert
            .execute(params![uid, id, amount])
            .map_err(|error| database_error(&error))?;
    }
    Ok(())
}
