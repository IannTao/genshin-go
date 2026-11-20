use super::archive::{Pool, Rarity, Wish};
use std::collections::BTreeMap;

#[derive(Default)]
enum Boundary {
    #[default]
    None,
    FiveStar,
    UnknownRank,
}

/// Summarizes the saved records and the observed end of one wish pool's history.
#[derive(Default)]
pub(super) struct PoolStatistics {
    three: usize,
    four: usize,
    five: usize,
    unknown: usize,
    trailing_draws: usize,
    boundary: Boundary,
}

impl PoolStatistics {
    fn add(&mut self, rarity: Rarity) {
        let count = match rarity {
            Rarity::Three => &mut self.three,
            Rarity::Four => &mut self.four,
            Rarity::Five => &mut self.five,
            Rarity::Unknown => &mut self.unknown,
        };
        *count += 1;
        if matches!(self.boundary, Boundary::None) {
            match rarity {
                Rarity::Three | Rarity::Four => self.trailing_draws += 1,
                Rarity::Five => self.boundary = Boundary::FiveStar,
                Rarity::Unknown => self.boundary = Boundary::UnknownRank,
            }
        }
    }

    /// Formats the counts of distinct imported draws by rarity.
    pub(super) fn rarity_label(&self, pool: Pool) -> String {
        let Self {
            three,
            four,
            five,
            unknown,
            ..
        } = *self;
        let total = three + four + five + unknown;
        format!(
            "{} · 已导入 {total} 抽 · 5 星 {five} · 4 星 {four} · 3 星 {three} · 星级未知 {unknown}",
            pool.label()
        )
    }

    /// Describes an estimate or the observed limit of the available guarantee history.
    pub(super) fn pity_label(&self, pool: Pool) -> String {
        let limit = match pool {
            Pool::Character | Pool::Standard | Pool::ChronicledAndLightrace => 90,
            Pool::Weapon => 80,
            Pool::Beginner => return "新手祈愿最多 20 抽，两次十连各有角色保底".into(),
        };
        let draws = self.trailing_draws;
        if draws >= limit {
            return format!("五星保底进度未知（{limit} 抽上限）· 已导入末尾记录与保底规则不符");
        }
        let reason = match self.boundary {
            Boundary::FiveStar => {
                return format!(
                    "五星保底估算 · 已累计 {draws} / {limit} 抽 · 最多再抽 {} 次",
                    limit - draws
                );
            }
            Boundary::UnknownRank => "近期有星级未知记录",
            Boundary::None => "缺少五星记录",
        };
        if draws == 0 {
            format!("五星保底进度未知（{limit} 抽上限）· {reason}")
        } else {
            format!("五星保底进度未知（{limit} 抽上限）· {reason} · 已导入末尾 {draws} 抽未出五星")
        }
    }
}

/// Summarizes all saved wishes for one UID in newest-first order.
pub(super) fn summarize(records: &[Wish]) -> Result<BTreeMap<Pool, PoolStatistics>, String> {
    let mut pools = BTreeMap::<Pool, PoolStatistics>::new();
    for wish in records {
        let pool = wish.pool().ok_or("已保存的祈愿记录包含未知卡池")?;
        pools.entry(pool).or_default().add(wish.rarity());
    }
    Ok(pools)
}
