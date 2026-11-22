//! Local character and weapon growth plans and their material requirements.
mod store;
mod ui;
mod weapon;

use crate::profile::Profile;
use nsh_core::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::RangeInclusive, path::PathBuf, sync::OnceLock};

/// Creates UID-owned growth plans and manually entered material quantities.
pub(crate) const SCHEMA: &str = store::SCHEMA;

/// Creates UID-owned weapon growth plans.
pub(crate) const WEAPON_SCHEMA: &str = store::WEAPON_SCHEMA;

/// Renders the character and weapon growth planners on the UID page.
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
type ExperienceMaterial = (i32, &'static str, u32);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Character,
    Weapon,
}

impl Kind {
    fn experience_materials(self) -> &'static [ExperienceMaterial] {
        match self {
            Self::Character => &EXP_BOOKS,
            Self::Weapon => &weapon::ORES,
        }
    }
}

#[derive(Deserialize)]
struct Catalog {
    materials: BTreeMap<i32, String>,
    experience: Curves,
    #[serde(alias = "characters")]
    items: Vec<Item>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Curves {
    Character(Vec<u32>),
    Weapon([Vec<u32>; 5]),
}

#[derive(Deserialize)]
struct Item {
    key: String,
    name: String,
    #[serde(default)]
    ascension: Vec<Cost>,
    #[serde(flatten)]
    growth: Growth,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Growth {
    Weapon {
        rarity: u8,
        #[serde(default)]
        quest_levels: Vec<u8>,
    },
    Character {
        #[serde(default)]
        wonderland: bool,
        #[serde(default)]
        talents: Vec<Cost>,
    },
}

impl Catalog {
    fn load(kind: Kind) -> Result<&'static Self, String> {
        static CHARACTERS: OnceLock<Result<Catalog, serde_json::Error>> = OnceLock::new();
        static WEAPONS: OnceLock<Result<Catalog, serde_json::Error>> = OnceLock::new();
        let (catalog, source) = match kind {
            Kind::Character => (
                &CHARACTERS,
                include_str!("../assets/character-growth-zh-cn.json"),
            ),
            Kind::Weapon => (&WEAPONS, include_str!("../assets/weapon-growth-zh-cn.json")),
        };
        catalog
            .get_or_init(|| serde_json::from_str(source))
            .as_ref()
            .map_err(|error| format!("无法读取养成材料目录：{error}"))
    }

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
    #[serde(skip_serializing_if = "Option::is_none")]
    talents: Option<[u8; 3]>,
}

impl Progress {
    fn phase_level(self, levels: &[u8]) -> Result<u8, String> {
        let phase = usize::from(self.phase);
        levels
            .get(phase)
            .copied()
            .ok_or_else(|| format!("突破阶段须为 0 至 {}", levels.len() - 1))
    }

    fn level_bounds(self, caps: &[u8]) -> Result<RangeInclusive<u8>, String> {
        let cap = self.phase_level(caps)?;
        let phase = usize::from(self.phase);
        let min = phase.checked_sub(1).map_or(1, |previous| caps[previous]);
        Ok(min..=cap)
    }

    fn validate_character(self, wonderland: bool) -> Result<[u8; 3], String> {
        let bounds = self.level_bounds(&LEVEL_CAPS)?;
        let (min, cap) = (*bounds.start(), *bounds.end());
        let phase = usize::from(self.phase);
        let talents = self.talents.ok_or("缺少角色的基础天赋等级")?;
        let beyond_cap = phase == 6 && matches!(self.level, 95 | 100);
        if wonderland {
            if !(self.level == cap || beyond_cap) || talents != [TALENT_CAPS[phase]; 3] {
                return Err(format!(
                    "奇偶在此突破阶段的等级须为 {cap}{}，三项天赋须为 {}",
                    if phase == 6 { "、95 或 100" } else { "" },
                    TALENT_CAPS[phase]
                ));
            }
        } else {
            if !(beyond_cap || bounds.contains(&self.level)) {
                return Err(format!(
                    "此突破阶段的等级须为 {min} 至 {cap}{}",
                    if phase == 6 { "、95 或 100" } else { "" }
                ));
            }
            let talent_cap = TALENT_CAPS[phase];
            if talents
                .iter()
                .any(|level| !(1..=talent_cap).contains(level))
            {
                return Err(format!("此突破阶段的基础天赋等级须为 1 至 {talent_cap}"));
            }
        }
        Ok(talents)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Plan {
    current: Progress,
    target: Progress,
}

impl Plan {
    fn new(item: &Item) -> Self {
        let initial = Progress {
            level: if matches!(
                item.growth,
                Growth::Character {
                    wonderland: true,
                    ..
                }
            ) {
                20
            } else {
                1
            },
            phase: 0,
            talents: matches!(item.growth, Growth::Character { .. }).then_some([1; 3]),
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
    rounded: u32,
}

impl Experience {
    fn for_levels(
        curve: &[u32],
        current: u8,
        target: u8,
        caps: &[u8],
        unit: u32,
    ) -> Result<Self, String> {
        let mut result = Self::default();
        let mut from = current;
        let ends = caps
            .iter()
            .copied()
            .filter(|&cap| cap > from && cap < target)
            .collect::<Vec<_>>();
        for to in ends.into_iter().chain(std::iter::once(target)) {
            let step = curve
                .get(usize::from(to - 1))
                .zip(curve.get(usize::from(from - 1)))
                .and_then(|(end, start)| end.checked_sub(*start))
                .ok_or("缺少对应的经验等级数据")?;
            result.needed += step;
            // EXP in excess of a cap can only be recovered in whole material units.
            result.rounded += step.next_multiple_of(unit);
            from = to;
        }
        Ok(result)
    }

    fn shortfall(
        self,
        materials: &[ExperienceMaterial],
        mut quantity: impl FnMut(i32) -> Result<u32, String>,
    ) -> Result<u64, String> {
        let available = materials
            .iter()
            .try_fold(0_u64, |total, &(id, _, experience)| {
                Ok::<_, String>(total + u64::from(experience) * u64::from(quantity(id)?))
            })?;
        Ok(u64::from(self.rounded).saturating_sub(available))
    }
}

impl Requirements {
    fn add(&mut self, id: i32, count: u32) {
        if count > 0 {
            *self.materials.entry(id).or_default() += count;
        }
    }

    fn add_steps(&mut self, costs: &[Cost], range: std::ops::Range<usize>) -> Result<(), String> {
        for step in costs.get(range).ok_or("缺少对应的突破或天赋材料数据")? {
            for &(id, count) in step {
                self.add(id, count);
            }
        }
        Ok(())
    }
}

fn requirements(catalog: &Catalog, item: &Item, plan: Plan) -> Result<Requirements, String> {
    match (&catalog.experience, &item.growth) {
        (
            Curves::Weapon(curves),
            Growth::Weapon {
                rarity,
                quest_levels,
            },
        ) => weapon::requirements(curves, *rarity, quest_levels, &item.ascension, plan),
        (
            Curves::Character(curve),
            Growth::Character {
                wonderland,
                talents,
            },
        ) => character_requirements(curve, &item.ascension, *wonderland, talents, plan),
        _ => Err("养成目录中的经验数据与计划类型不匹配".into()),
    }
}

fn character_requirements(
    curve: &[u32],
    ascension: &[Cost],
    wonderland: bool,
    talents: &[Cost],
    plan: Plan,
) -> Result<Requirements, String> {
    let Plan { current, target } = plan;
    let current_talents = current
        .validate_character(wonderland)
        .map_err(|error| format!("当前状态：{error}"))?;
    let target_talents = target
        .validate_character(wonderland)
        .map_err(|error| format!("目标状态：{error}"))?;
    if target.level < current.level
        || target.phase < current.phase
        || current_talents
            .into_iter()
            .zip(target_talents)
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
    if wonderland {
        return Ok(needed);
    }
    needed.experience = Experience::for_levels(
        curve,
        current.level.min(90),
        target.level.min(90),
        &LEVEL_CAPS,
        1_000,
    )?;
    needed.add(MORA, needed.experience.rounded / 5);
    needed.add_steps(
        ascension,
        usize::from(current.phase)..usize::from(target.phase),
    )?;
    for (from, to) in current_talents.into_iter().zip(target_talents) {
        needed.add_steps(talents, usize::from(from - 1)..usize::from(to - 1))?;
    }
    Ok(needed)
}
