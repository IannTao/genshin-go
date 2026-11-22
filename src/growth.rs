//! Local character growth plans and their material requirements.
mod store;
mod ui;

use crate::profile::Profile;
use nsh_core::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};

/// Creates UID-owned growth plans and manually entered material quantities.
pub(crate) const SCHEMA: &str = store::SCHEMA;

/// Renders the character growth planner on the UID page.
pub(crate) fn section(
    path: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    ui::section(path, profile, revision)
}

const LEVEL_CAPS: [u8; 7] = [20, 40, 50, 60, 70, 80, 90];
const TALENT_CAPS: [u8; 7] = [1, 1, 2, 4, 6, 8, 10];
const MORA: i32 = 202;
const STELLA_FORTUNA: i32 = 104_300;
const EXP_BOOKS: [(i32, &str, u32); 3] = [
    (104_003, "大英雄的经验", 20_000),
    (104_002, "冒险家的经验", 5_000),
    (104_001, "流浪者的经验", 1_000),
];

type MaterialCounts = BTreeMap<i32, u32>;
type Cost = Vec<(i32, u32)>;

#[derive(Deserialize)]
struct Catalog {
    materials: BTreeMap<i32, String>,
    experience: Vec<u32>,
    characters: Vec<Character>,
}

#[derive(Deserialize)]
struct Character {
    key: String,
    name: String,
    #[serde(default)]
    wonderland: bool,
    #[serde(default)]
    ascension: Vec<Cost>,
    #[serde(default)]
    talents: Vec<Cost>,
}

fn catalog() -> Result<&'static Catalog, String> {
    static CATALOG: OnceLock<Result<Catalog, serde_json::Error>> = OnceLock::new();
    CATALOG
        .get_or_init(|| serde_json::from_str(include_str!("../assets/character-growth-zh-cn.json")))
        .as_ref()
        .map_err(|error| format!("无法读取角色材料目录：{error}"))
}

impl Catalog {
    fn material_name(&self, id: i32) -> &str {
        if id == STELLA_FORTUNA {
            "无主的命星"
        } else {
            self.materials.get(&id).map_or("未知材料", String::as_str)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Progress {
    level: u8,
    phase: u8,
    talents: [u8; 3],
}

impl Progress {
    fn validate(self, wonderland: bool) -> Result<(), String> {
        let phase = usize::from(self.phase);
        let cap = *LEVEL_CAPS.get(phase).ok_or("突破阶段须为 0 至 6")?;
        let min = phase
            .checked_sub(1)
            .map_or(1, |previous| LEVEL_CAPS[previous]);
        let beyond_cap = phase == 6 && matches!(self.level, 95 | 100);
        if wonderland {
            if !(self.level == cap || beyond_cap) || self.talents != [TALENT_CAPS[phase]; 3] {
                return Err(format!(
                    "奇偶在此突破阶段的等级须为 {cap}{}，三项天赋须为 {}",
                    if phase == 6 { "、95 或 100" } else { "" },
                    TALENT_CAPS[phase]
                ));
            }
        } else {
            if !(beyond_cap || (min..=cap).contains(&self.level)) {
                return Err(format!(
                    "此突破阶段的等级须为 {min} 至 {cap}{}",
                    if phase == 6 { "、95 或 100" } else { "" }
                ));
            }
            let talent_cap = TALENT_CAPS[phase];
            if self
                .talents
                .iter()
                .any(|level| !(1..=talent_cap).contains(level))
            {
                return Err(format!("此突破阶段的基础天赋等级须为 1 至 {talent_cap}"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Plan {
    current: Progress,
    target: Progress,
}

impl Plan {
    fn new(character: &Character) -> Self {
        let initial = Progress {
            level: if character.wonderland { 20 } else { 1 },
            phase: 0,
            talents: [1; 3],
        };
        Self {
            current: initial,
            target: initial,
        }
    }
}

#[derive(Default)]
struct Requirements {
    experience: Experience,
    materials: MaterialCounts,
}

#[derive(Clone, Copy, Default)]
struct Experience {
    needed: u32,
    book_experience: u32,
}

impl Experience {
    fn for_levels(curve: &[u32], current: u8, target: u8) -> Result<Self, String> {
        let mut result = Self::default();
        let mut from = current.min(90);
        let target = target.min(90);
        let ends = LEVEL_CAPS
            .into_iter()
            .filter(|&cap| cap > from && cap < target)
            .collect::<Vec<_>>();
        for to in ends.into_iter().chain(std::iter::once(target)) {
            let step = curve
                .get(usize::from(to - 1))
                .zip(curve.get(usize::from(from - 1)))
                .and_then(|(end, start)| end.checked_sub(*start))
                .ok_or("缺少角色经验等级数据")?;
            result.needed += step;
            // EXP in excess of a cap can only be recovered in whole book units.
            result.book_experience += step.next_multiple_of(1_000);
            from = to;
        }
        Ok(result)
    }

    fn shortfall(
        self,
        mut quantity: impl FnMut(i32) -> Result<u32, String>,
    ) -> Result<u64, String> {
        let available = EXP_BOOKS
            .into_iter()
            .try_fold(0_u64, |total, (id, _, experience)| {
                Ok::<_, String>(total + u64::from(experience) * u64::from(quantity(id)?))
            })?;
        Ok(u64::from(self.book_experience).saturating_sub(available))
    }
}

impl Requirements {
    fn add(&mut self, id: i32, count: u32) {
        if count > 0 {
            *self.materials.entry(id).or_default() += count;
        }
    }

    fn add_steps(&mut self, costs: &[Cost], range: std::ops::Range<usize>) -> Result<(), String> {
        for step in costs.get(range).ok_or("此角色缺少对应的材料数据")? {
            for &(id, count) in step {
                self.add(id, count);
            }
        }
        Ok(())
    }
}

fn requirements(
    catalog: &Catalog,
    character: &Character,
    plan: Plan,
) -> Result<Requirements, String> {
    let Plan { current, target } = plan;
    current
        .validate(character.wonderland)
        .map_err(|error| format!("当前状态：{error}"))?;
    target
        .validate(character.wonderland)
        .map_err(|error| format!("目标状态：{error}"))?;
    if target.level < current.level
        || target.phase < current.phase
        || current
            .talents
            .into_iter()
            .zip(target.talents)
            .any(|(from, to)| to < from)
    {
        return Err("目标等级、突破阶段和各项天赋都不能低于当前状态".into());
    }

    let mut needed = Requirements::default();
    let stella = |level| match level {
        100 => 3,
        95 => 1,
        _ => 0,
    };
    needed.add(STELLA_FORTUNA, stella(target.level) - stella(current.level));
    if character.wonderland {
        return Ok(needed);
    }
    needed.experience = Experience::for_levels(&catalog.experience, current.level, target.level)?;
    needed.add(MORA, needed.experience.book_experience / 5);
    needed.add_steps(
        &character.ascension,
        usize::from(current.phase)..usize::from(target.phase),
    )?;
    for (from, to) in current.talents.into_iter().zip(target.talents) {
        needed.add_steps(
            &character.talents,
            usize::from(from - 1)..usize::from(to - 1),
        )?;
    }
    Ok(needed)
}
