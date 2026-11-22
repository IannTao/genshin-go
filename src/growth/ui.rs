use super::{
    Catalog, Character, EXP_BOOKS, Experience, MaterialCounts, Plan, Progress, Requirements,
    catalog, requirements, store,
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
    let expanded = signal(false);
    let query = signal(String::new());
    let active_uid = computed(move || profile.get().active_uid);
    view()
        .gap_2()
        .child(button("角色养成计划", move || {
            expanded.update(|open| *open = !*open);
        }))
        .child(dynamic(move || {
            expanded.get().then(|| {
                let Some(uid) = active_uid.get() else {
                    return view().child(text("请先创建或选择游戏 UID"));
                };
                revision.get();
                let data = database
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|path| Ok((catalog()?, store::load(path, &uid)?, path.clone())));
                match data {
                    Ok((catalog, saved, path)) => planner(catalog, saved, &path, &uid, query),
                    Err(error) => view().child(text(error)),
                }
            })
        }))
}

#[derive(Clone)]
struct Planner {
    catalog: &'static Catalog,
    path: Rc<PathBuf>,
    uid: Rc<str>,
    plans: Signal<BTreeMap<String, Plan>>,
    stock: Signal<MaterialCounts>,
    selected: Signal<Option<usize>>,
}

fn planner(
    catalog: &'static Catalog,
    saved: store::Saved,
    path: &std::path::Path,
    uid: &str,
    query: Signal<String>,
) -> View {
    let planner = Planner {
        catalog,
        path: Rc::new(path.to_owned()),
        uid: Rc::from(uid),
        plans: signal(saved.plans),
        stock: signal(saved.stock),
        selected: signal(None),
    };
    let edit = planner.clone();
    view()
        .gap_2()
        .child(text(format!("UID {uid} 的角色养成计划")).font_weight(FontWeight::BOLD))
        .child(dynamic(move || {
            edit.selected
                .get()
                .map(|index| editor(&edit, &catalog.characters[index]))
        }))
        .child(planner.saved_plans())
        .child(planner.character_picker(query))
}

impl Planner {
    fn saved_plans(&self) -> impl IntoElement + use<> {
        let plans = self.plans;
        let selected = self.selected;
        let catalog = self.catalog;
        dynamic(move || {
            plans.with(|plans| {
                let buttons = catalog
                    .characters
                    .iter()
                    .enumerate()
                    .filter_map(|(index, character)| {
                        let plan = plans.get(&character.key)?;
                        Some(
                            button(
                                format!(
                                    "查看{}的计划（等级 {} → {}）",
                                    character.name, plan.current.level, plan.target.level
                                ),
                                move || selected.set(Some(index)),
                            )
                            .into_element(),
                        )
                    })
                    .collect::<Vec<_>>();
                view().gap_2().child(if buttons.is_empty() {
                    text("暂无角色养成计划").into_element()
                } else {
                    button_row().children(buttons).into_element()
                })
            })
        })
    }

    fn character_picker(&self, query: Signal<String>) -> View {
        let page = signal(0_usize);
        let selected = self.selected;
        let catalog = self.catalog;
        view()
            .gap_2()
            .child(
                field("growth-search", "搜索角色名称或 ID")
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
                    .characters
                    .iter()
                    .enumerate()
                    .filter(|(_, character)| {
                        character.name.contains(query) || character.key.contains(query)
                    })
                    .collect::<Vec<_>>();
                let page_index = page.get();
                let choices = matches
                    .iter()
                    .skip(page_index * PAGE_SIZE)
                    .take(PAGE_SIZE)
                    .map(|&(index, character)| {
                        button(format!("规划{}", character.name), move || {
                            selected.set(Some(index));
                        })
                        .into_element()
                    })
                    .collect::<Vec<_>>();
                view()
                    .gap_2()
                    .child(choices.is_empty().then(|| text("没有匹配的角色")))
                    .child(button_row().children(choices))
                    .child(
                        button_row()
                            .child((page_index > 0).then(|| {
                                button("上一页角色", move || page.set(page_index - 1))
                            }))
                            .child(((page_index + 1) * PAGE_SIZE < matches.len()).then(|| {
                                button("下一页角色", move || page.set(page_index + 1))
                            })),
                    )
            }))
    }
}

#[derive(Clone, Copy)]
struct Draft {
    current: Signal<[String; 5]>,
    target: Signal<[String; 5]>,
    stock_edits: Signal<BTreeMap<i32, String>>,
    message: Signal<String>,
}

fn editor(planner: &Planner, character: &'static Character) -> View {
    let plan = planner
        .plans
        .with_untracked(|plans| plans.get(&character.key).copied())
        .unwrap_or_else(|| Plan::new(character));
    let display = |state: Progress| {
        [
            state.level,
            state.phase,
            state.talents[0],
            state.talents[1],
            state.talents[2],
        ]
        .map(|number| number.to_string())
    };
    let draft = Draft {
        current: signal(display(plan.current)),
        target: signal(display(plan.target)),
        stock_edits: signal(BTreeMap::new()),
        message: signal(String::new()),
    };
    let preview = planner.clone();
    let selected = planner.selected;
    view()
        .gap_2()
        .child(text(format!("{}的养成计划", character.name)).font_weight(FontWeight::BOLD))
        .child(text(if character.wonderland {
            "达到千星奇域等级 10 后可突破奇偶，各阶段自动提升至对应等级与天赋。"
        } else {
            "突破阶段填已完成的次数（0 至 6）。天赋填基础等级。"
        }))
        .child(progress_fields(
            "current",
            "当前",
            draft.current,
            draft.message,
        ))
        .child(progress_fields(
            "target",
            "目标",
            draft.target,
            draft.message,
        ))
        .child(dynamic(move || {
            match draft
                .plan()
                .and_then(|plan| requirements(preview.catalog, character, plan))
            {
                Ok(needed) => materials(preview.catalog, &needed, preview.stock, draft),
                Err(error) => view().child(text(error)),
            }
        }))
        .child(
            button_row()
                .child(button("保存养成计划", {
                    let planner = planner.clone();
                    move || save(&planner, character, draft)
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
            current: self.current.with(|fields| parse_progress("当前", fields))?,
            target: self.target.with(|fields| parse_progress("目标", fields))?,
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

fn parse_progress(state: &str, fields: &[String; 5]) -> Result<Progress, String> {
    let mut numbers = [0; 5];
    for (index, value) in fields.iter().enumerate() {
        numbers[index] = value
            .trim()
            .parse()
            .map_err(|_| format!("请为{state}{}输入有效整数", PROGRESS_FIELDS[index].1))?;
    }
    let [level, phase, normal, skill, burst] = numbers;
    Ok(Progress {
        level,
        phase,
        talents: [normal, skill, burst],
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

fn save(planner: &Planner, character: &Character, draft: Draft) {
    let result = (|| {
        let plan = draft.plan()?;
        let needed = requirements(planner.catalog, character, plan)?;
        let stock = draft.stock_edits.with_untracked(|edits| {
            edits
                .iter()
                .filter(|&(&id, _)| {
                    needed.materials.contains_key(&id)
                        || (needed.experience.needed > 0
                            && EXP_BOOKS.iter().any(|&(book, _, _)| book == id))
                })
                .map(|(&id, value)| Ok((id, parse_quantity(value)?)))
                .collect::<Result<MaterialCounts, String>>()
        })?;
        store::save(&planner.path, &planner.uid, &character.key, plan, &stock)?;
        planner.plans.update(|plans| {
            plans.insert(character.key.clone(), plan);
        });
        planner.stock.update(|counts| counts.extend(stock));
        draft.stock_edits.set(BTreeMap::new());
        Ok::<_, String>(())
    })();
    draft.message.set(match result {
        Ok(()) => format!("已保存{}的养成计划", character.name),
        Err(error) => error,
    });
}

fn progress_fields(
    key: &str,
    state: &str,
    fields: Signal<[String; 5]>,
    message: Signal<String>,
) -> View {
    button_row().children(
        PROGRESS_FIELDS
            .iter()
            .enumerate()
            .map(|(index, (field_key, label))| {
                let label = format!("{state}{label}");
                view()
                    .gap_1()
                    .child(text(label.clone()))
                    .child(
                        field(format!("growth-{key}-{field_key}"), label)
                            .w(px(110.0))
                            .value(move || fields.with(|fields| fields[index].clone()))
                            .on_change(move |value: &str| {
                                fields.update(|fields| value.clone_into(&mut fields[index]));
                                message.set(String::new());
                            }),
                    )
                    .into_element()
            }),
    )
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
    view()
        .gap_2()
        .child(text(format!("角色经验 · 需求 {}", experience.needed)))
        .child(text(move || {
            match experience.shortfall(|id| draft.quantity(id, stock)) {
                Ok(shortfall) => format!("经验书折算 · 需求 {} · 缺口 {shortfall}", experience.book_experience),
                Err(error) => format!("经验书折算 · {error}"),
            }
        }))
        .child(text(
            "跨突破时按各段向上取整到 1000 经验。升级摩拉按折算量估算，实际用量受已有经验和用书方式影响。",
        ))
        .child(
            button_row().children(EXP_BOOKS.into_iter().map(|(id, name, exp)| {
                view()
                    .gap_1()
                    .child(text(format!("{name}（每本 {exp}）")))
                    .child(stock_field(id, name, stock, draft))
                    .into_element()
            })),
        )
}

fn stock_field(id: i32, name: &str, stock: Signal<MaterialCounts>, draft: Draft) -> Input {
    field(format!("growth-stock-{id}"), format!("{name}手填库存"))
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
