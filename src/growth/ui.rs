use super::{
    Catalog, Experience, Growth, Item, Kind, MaterialCounts, Plan, Progress, Requirements,
    requirements, store, weapon,
};
use crate::{launcher::button, profile::Profile};
use nsh_base::theme;
use nsh_core::{element::input, prelude::*};
use std::{collections::BTreeMap, path::PathBuf, rc::Rc};

const PAGE_SIZE: usize = 8;
const PROGRESS_FIELDS: [(&str, &str); 5] = [
    ("level", "等级"),
    ("phase", "突破阶段"),
    ("normal", "普通攻击"),
    ("skill", "元素战技"),
    ("burst", "元素爆发"),
];

/// Adds a planner that reloads when the active UID or its history changes.
pub(super) fn section(
    database: Result<PathBuf, String>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> View {
    let expanded = signal(None);
    let character_query = signal(String::new());
    let weapon_query = signal(String::new());
    let active_uid = computed(move || profile.get().active_uid);
    view()
        .gap_2()
        .child(
            button_row().children([Kind::Character, Kind::Weapon].map(|kind| {
                button(format!("{}养成计划", kind.subject()), move || {
                    expanded.update(|open| *open = (*open != Some(kind)).then_some(kind));
                })
                .into_element()
            })),
        )
        .child(dynamic(move || {
            expanded.get().map(|kind| {
                let Some(uid) = active_uid.get() else {
                    return view().child(text("请先创建或选择游戏 UID"));
                };
                revision.get();
                let query = match kind {
                    Kind::Character => character_query,
                    Kind::Weapon => weapon_query,
                };
                let data = database.as_ref().map_err(Clone::clone).and_then(|path| {
                    Ok((Catalog::load(kind)?, store::load(path, &uid, kind)?, path))
                });
                match data {
                    Ok((catalog, saved, path)) => planner(kind, catalog, saved, path, &uid, query),
                    Err(error) => view().child(text(error)),
                }
            })
        }))
}

impl Kind {
    fn subject(self) -> &'static str {
        match self {
            Self::Character => "角色",
            Self::Weapon => "武器",
        }
    }

    fn field_prefix(self) -> &'static str {
        match self {
            Self::Character => "growth",
            Self::Weapon => "weapon-growth",
        }
    }

    fn progress_fields(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Character => &PROGRESS_FIELDS,
            Self::Weapon => &PROGRESS_FIELDS[..2],
        }
    }
}

#[derive(Clone)]
struct Planner {
    kind: Kind,
    catalog: &'static Catalog,
    path: Rc<PathBuf>,
    uid: Rc<str>,
    plans: Signal<BTreeMap<String, Plan>>,
    stock: Signal<MaterialCounts>,
    selected: Signal<Option<usize>>,
}

fn planner(
    kind: Kind,
    catalog: &'static Catalog,
    saved: store::Saved,
    path: &std::path::Path,
    uid: &str,
    query: Signal<String>,
) -> View {
    let planner = Planner {
        kind,
        catalog,
        path: Rc::new(path.to_owned()),
        uid: Rc::from(uid),
        plans: signal(saved.plans),
        stock: signal(saved.stock),
        selected: signal(None),
    };
    let edit = planner.clone();
    view()
        .key(format!("{}-{uid}", kind.field_prefix()))
        .gap_2()
        .child(
            text(format!("UID {uid} 的{}养成计划", kind.subject())).font_weight(FontWeight::BOLD),
        )
        .child(dynamic(move || {
            edit.selected
                .get()
                .map(|index| editor(&edit, &catalog.items[index]))
        }))
        .child(planner.saved_plans())
        .child(planner.item_picker(query))
}

impl Planner {
    fn saved_plans(&self) -> impl IntoElement + use<> {
        let plans = self.plans;
        let selected = self.selected;
        let catalog = self.catalog;
        let subject = self.kind.subject();
        dynamic(move || {
            plans.with(|plans| {
                let buttons = catalog
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, item)| {
                        let plan = plans.get(&item.key)?;
                        Some(
                            button(
                                format!(
                                    "查看{}的计划（等级 {} → {}）",
                                    item.name, plan.current.level, plan.target.level
                                ),
                                move || selected.set(Some(index)),
                            )
                            .into_element(),
                        )
                    })
                    .collect::<Vec<_>>();
                view().gap_2().child(if buttons.is_empty() {
                    text(format!("暂无{subject}养成计划")).into_element()
                } else {
                    button_row().children(buttons).into_element()
                })
            })
        })
    }

    fn item_picker(&self, query: Signal<String>) -> View {
        let page = signal(0_usize);
        let selected = self.selected;
        let catalog = self.catalog;
        let subject = self.kind.subject();
        view()
            .gap_2()
            .child(
                field(
                    format!("{}-search", self.kind.field_prefix()),
                    format!("搜索{subject}名称或 ID"),
                )
                .value(move || query.get())
                .on_change(move |value: &str| {
                    page.set(0);
                    query.set(value.to_owned());
                }),
            )
            .child(dynamic(move || {
                let query = query.get();
                let query = query.trim();
                let matches = catalog
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item.name.contains(query) || item.key.contains(query))
                    .collect::<Vec<_>>();
                let page_index = page.get();
                let choices = matches
                    .iter()
                    .skip(page_index * PAGE_SIZE)
                    .take(PAGE_SIZE)
                    .map(|&(index, item)| {
                        button(format!("规划{}", item.name), move || {
                            selected.set(Some(index));
                        })
                        .into_element()
                    })
                    .collect::<Vec<_>>();
                view()
                    .gap_2()
                    .child(
                        choices
                            .is_empty()
                            .then(|| text(format!("没有匹配的{subject}"))),
                    )
                    .child(button_row().children(choices))
                    .child(
                        button_row()
                            .child((page_index > 0).then(|| {
                                button(format!("上一页{subject}"), move || {
                                    page.set(page_index - 1);
                                })
                            }))
                            .child(((page_index + 1) * PAGE_SIZE < matches.len()).then(|| {
                                button(format!("下一页{subject}"), move || {
                                    page.set(page_index + 1);
                                })
                            })),
                    )
            }))
    }
}

#[derive(Clone, Copy)]
struct Draft {
    kind: Kind,
    current: Signal<[String; 5]>,
    target: Signal<[String; 5]>,
    stock_edits: Signal<BTreeMap<i32, String>>,
    message: Signal<String>,
}

fn editor(planner: &Planner, item: &'static Item) -> View {
    let plan = planner
        .plans
        .with_untracked(|plans| plans.get(&item.key).copied())
        .unwrap_or_else(|| Plan::new(item));
    let display = |state: Progress| {
        let [normal, skill, burst] = state.talents.unwrap_or_default();
        [state.level, state.phase, normal, skill, burst].map(|number| number.to_string())
    };
    let draft = Draft {
        kind: planner.kind,
        current: signal(display(plan.current)),
        target: signal(display(plan.target)),
        stock_edits: signal(BTreeMap::new()),
        message: signal(String::new()),
    };
    let preview = planner.clone();
    let selected = planner.selected;
    view()
        .key(item.key.clone())
        .gap_2()
        .child(text(format!("{}的养成计划", item.name)).font_weight(FontWeight::BOLD))
        .child(text(match &item.growth {
            Growth::Character {
                wonderland: true, ..
            } => "达到千星奇域等级 10 后可突破奇偶，各阶段自动提升至对应等级与天赋。".to_owned(),
            Growth::Character { .. } => {
                "突破阶段填已完成的次数（0 至 6）。天赋填基础等级。".to_owned()
            }
            Growth::Weapon { quest_levels, .. } if !quest_levels.is_empty() => format!(
                "随魔神任务与至冬七天神像解锁等级。阶段 0 至 {} 依次对应等级 {}。",
                quest_levels.len() - 1,
                quest_levels
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join("、"),
            ),
            Growth::Weapon { rarity, .. } => {
                let caps = weapon::level_caps(*rarity);
                let phase = caps.len() - 1;
                format!(
                    "突破阶段填已完成的次数（0 至 {phase}）。最高等级为 {}。",
                    caps[phase]
                )
            }
        }))
        .child(progress_fields("current", "当前", draft.current, draft))
        .child(progress_fields("target", "目标", draft.target, draft))
        .child(dynamic(move || {
            match draft
                .plan()
                .and_then(|plan| requirements(preview.catalog, item, plan))
            {
                Ok(needed) => materials(preview.catalog, &needed, preview.stock, draft),
                Err(error) => view().child(text(error)),
            }
        }))
        .child(
            button_row()
                .child(button("保存养成计划", {
                    let planner = planner.clone();
                    move || save(&planner, item, draft)
                }))
                .child(button("关闭计划编辑", move || {
                    selected.set(None);
                })),
        )
        .child(text(move || draft.message.get()))
}

impl Draft {
    fn plan(self) -> Result<Plan, String> {
        Ok(Plan {
            current: self
                .current
                .with(|fields| parse_progress(self.kind, "当前", fields))?,
            target: self
                .target
                .with(|fields| parse_progress(self.kind, "目标", fields))?,
        })
    }

    fn quantity(self, id: i32, stock: Signal<MaterialCounts>) -> Result<u32, String> {
        self.stock_edits.with(|edits| {
            edits.get(&id).map_or_else(
                || Ok(stock.with(|counts| counts.get(&id).copied().unwrap_or_default())),
                |value| parse_quantity(value),
            )
        })
    }
}

fn parse_progress(kind: Kind, state: &str, fields: &[String; 5]) -> Result<Progress, String> {
    let mut numbers = [0; 5];
    for ((number, value), (_, label)) in numbers.iter_mut().zip(fields).zip(kind.progress_fields())
    {
        *number = value
            .trim()
            .parse()
            .map_err(|_| format!("请为{state}{label}输入有效整数"))?;
    }
    let [level, phase, normal, skill, burst] = numbers;
    Ok(Progress {
        level,
        phase,
        talents: (kind == Kind::Character).then_some([normal, skill, burst]),
    })
}

fn parse_quantity(value: &str) -> Result<u32, String> {
    if value.trim().is_empty() {
        return Ok(0);
    }
    value
        .trim()
        .parse()
        .map_err(|_| "库存须为 0 至 4294967295 的整数".into())
}

fn save(planner: &Planner, item: &Item, draft: Draft) {
    let result = (|| {
        let plan = draft.plan()?;
        let needed = requirements(planner.catalog, item, plan)?;
        let stock = draft.stock_edits.with_untracked(|edits| {
            edits
                .iter()
                .filter(|&(&id, _)| {
                    needed.materials.contains_key(&id)
                        || (needed.experience.needed > 0
                            && draft
                                .kind
                                .experience_materials()
                                .iter()
                                .any(|&(material, _, _)| material == id))
                })
                .map(|(&id, value)| Ok((id, parse_quantity(value)?)))
                .collect::<Result<MaterialCounts, String>>()
        })?;
        store::save(
            &planner.path,
            &planner.uid,
            planner.kind,
            &item.key,
            plan,
            &stock,
        )?;
        planner.plans.update(|plans| {
            plans.insert(item.key.clone(), plan);
        });
        planner.stock.update(|counts| counts.extend(stock));
        draft.stock_edits.set(BTreeMap::new());
        Ok::<_, String>(())
    })();
    draft.message.set(match result {
        Ok(()) => format!("已保存{}的养成计划", item.name),
        Err(error) => error,
    });
}

fn progress_fields(key: &str, state: &str, fields: Signal<[String; 5]>, draft: Draft) -> View {
    button_row().children(draft.kind.progress_fields().iter().enumerate().map(
        |(index, (field_key, label))| {
            let label = format!("{state}{label}");
            view()
                .gap_1()
                .child(text(label.clone()))
                .child(
                    field(
                        format!("{}-{key}-{field_key}", draft.kind.field_prefix()),
                        label,
                    )
                    .w(px(110.0))
                    .value(move || fields.with(|fields| fields[index].clone()))
                    .on_change(move |value: &str| {
                        fields.update(|fields| value.clone_into(&mut fields[index]));
                        draft.message.set(String::new());
                    }),
                )
                .into_element()
        },
    ))
}

fn materials(
    catalog: &Catalog,
    needed: &Requirements,
    stock: Signal<MaterialCounts>,
    draft: Draft,
) -> View {
    let rows = needed
        .materials
        .iter()
        .map(|(&id, &amount)| {
            let name = catalog.material_name(id).to_owned();
            view()
                .gap_1()
                .child(text({
                    let name = name.clone();
                    move || match draft.quantity(id, stock) {
                        Ok(available) => format!(
                            "{name} · 需求 {amount} · 缺口 {}",
                            amount.saturating_sub(available)
                        ),
                        Err(error) => format!("{name} · {error}"),
                    }
                }))
                .child(stock_field(id, &name, stock, draft))
                .into_element()
        })
        .collect::<Vec<_>>();
    view()
        .gap_2()
        .child(rows.is_empty().then(|| text("无需养成材料")))
        .child(
            (needed.experience.needed > 0)
                .then(|| experience_fields(needed.experience, stock, draft)),
        )
        .child(button_row().children(rows))
}

fn experience_fields(experience: Experience, stock: Signal<MaterialCounts>, draft: Draft) -> View {
    let (label, unit, instruction) = match draft.kind {
        Kind::Character => (
            "经验书",
            "本",
            "跨突破时按各段向上取整到 1000 经验。升级摩拉按折算量估算，实际用量受已有经验和用书方式影响。".to_owned(),
        ),
        Kind::Weapon => (
            "强化矿",
            "个",
            format!("跨突破时按各段向上取整到 {} 经验。强化摩拉按折算量估算，实际用量受已有经验和用矿方式影响。", weapon::ORE_EXP_UNIT),
        ),
    };
    let materials = draft.kind.experience_materials();
    view()
        .gap_2()
        .child(text(format!(
            "{}经验 · 需求 {}",
            draft.kind.subject(),
            experience.needed
        )))
        .child(text(move || {
            match experience.shortfall(materials, |id| draft.quantity(id, stock)) {
                Ok(shortfall) => format!(
                    "{label}折算 · 需求 {} · 缺口 {shortfall}",
                    experience.rounded
                ),
                Err(error) => format!("{label}折算 · {error}"),
            }
        }))
        .child(text(instruction))
        .child(
            button_row().children(materials.iter().map(|&(id, name, exp)| {
                view()
                    .gap_1()
                    .child(text(format!("{name}（每{unit} {exp}）")))
                    .child(stock_field(id, name, stock, draft))
                    .into_element()
            })),
        )
}

fn stock_field(id: i32, name: &str, stock: Signal<MaterialCounts>, draft: Draft) -> Input {
    field(
        format!("{}-stock-{id}", draft.kind.field_prefix()),
        format!("{name}手填库存"),
    )
    .w(px(140.0))
    .placeholder("手填库存（默认 0）")
    .value(move || {
        draft
            .stock_edits
            .with(|edits| edits.get(&id).cloned())
            .unwrap_or_else(|| {
                stock.with(|counts| {
                    counts
                        .get(&id)
                        .filter(|&&count| count > 0)
                        .map_or_else(String::new, u32::to_string)
                })
            })
    })
    .on_change(move |value: &str| {
        draft.stock_edits.update(|edits| {
            edits.insert(id, value.to_owned());
        });
        draft.message.set(String::new());
    })
}

fn field(id: impl Into<String>, label: impl Into<String>) -> Input {
    let label = label.into();
    input()
        .test_id(&id.into())
        .label(label.clone())
        .placeholder(label)
        .h_10()
        .px_3()
        .bg(move || theme().secondary)
}

fn button_row() -> View {
    view().flex_row().flex_wrap().gap_2()
}
