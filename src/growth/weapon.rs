use super::{Cost, Experience, ExperienceMaterial, LEVEL_CAPS, MORA, Plan, Progress, Requirements};

/// Lists the enhancement ores that can cover weapon EXP requirements.
pub(super) const ORES: [ExperienceMaterial; 3] = [
    (104_013, "精锻用魔矿", 10_000),
    (104_012, "精锻用良矿", 2_000),
    (104_011, "精锻用杂矿", 400),
];

/// Gives the smallest unit of EXP available from enhancement ores.
pub(super) const ORE_EXP_UNIT: u32 = ORES[2].2;

/// Lists the maximum level at each standard ascension phase for the given rarity.
pub(super) fn level_caps(rarity: u8) -> &'static [u8] {
    &LEVEL_CAPS[..if rarity <= 2 { 5 } else { 7 }]
}

/// Calculates weapon enhancement and ascension costs between two valid states.
pub(super) fn requirements(
    curves: &[Vec<u32>; 5],
    rarity: u8,
    quest_levels: &[u8],
    ascension: &[Cost],
    plan: Plan,
) -> Result<Requirements, String> {
    let caps = level_caps(rarity);
    let Plan { current, target } = plan;
    validate(current, caps, quest_levels).map_err(|error| format!("当前状态：{error}"))?;
    validate(target, caps, quest_levels).map_err(|error| format!("目标状态：{error}"))?;
    if target.level < current.level || target.phase < current.phase {
        return Err("目标等级和突破阶段都不能低于当前状态".into());
    }
    if !quest_levels.is_empty() {
        return Ok(Requirements::default());
    }

    let curve = rarity
        .checked_sub(1)
        .and_then(|index| curves.get(usize::from(index)))
        .ok_or("缺少此星级的武器经验数据")?;
    let mut needed = Requirements {
        experience: Experience::for_levels(curve, current.level, target.level, caps, ORE_EXP_UNIT)?,
        ..Requirements::default()
    };
    needed.add(MORA, needed.experience.rounded / 10);
    needed.add_steps(
        ascension,
        usize::from(current.phase)..usize::from(target.phase),
    )?;
    Ok(needed)
}

fn validate(progress: Progress, caps: &[u8], quest_levels: &[u8]) -> Result<(), String> {
    if !quest_levels.is_empty() {
        let level = progress.phase_level(quest_levels)?;
        return if progress.level == level {
            Ok(())
        } else {
            Err(format!("此阶段的等级须为 {level}"))
        };
    }
    let bounds = progress.level_bounds(caps)?;
    if bounds.contains(&progress.level) {
        Ok(())
    } else {
        Err(format!(
            "此突破阶段的等级须为 {} 至 {}",
            bounds.start(),
            bounds.end()
        ))
    }
}
