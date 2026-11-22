use super::{archive::Record, store};
use crate::{launcher::button, profile::Profile};
use nsh_base::theme;
use nsh_core::element::input;
use nsh_core::prelude::*;
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, rc::Rc, sync::OnceLock};

const PAGE_SIZE: usize = 30;

#[derive(Deserialize)]
struct Catalog {
    version: String,
    entries: Vec<CatalogEntry>,
}

#[derive(Deserialize)]
struct CatalogEntry {
    id: u32,
    title: String,
    stage: Option<Stage>,
}

#[derive(Deserialize)]
struct Stage {
    number: u32,
    target: u32,
}

/// Renders the offline catalog and its saved progress for the selected UID.
pub(super) fn section(
    database: Rc<PathBuf>,
    profile: Signal<Profile>,
    revision: Signal<u64>,
) -> impl IntoElement {
    static CATALOG: OnceLock<Result<Catalog, serde_json::Error>> = OnceLock::new();
    let catalog = CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/achievement-catalog-zh-cn.json"))
    });
    let query = signal(String::new());
    let position = signal((String::new(), 0_usize));
    let message = signal(None::<(String, String)>);

    dynamic(move || {
        revision.get();
        let Some(uid) = profile.get().active_uid else {
            return view().child(text("请在角色资料中创建或选择游戏 UID"));
        };
        let content = view()
            .gap_2()
            .child(text(format!("UID {uid} 的成就进度")).font_weight(FontWeight::BOLD));
        let catalog = match catalog {
            Ok(catalog) => catalog,
            Err(error) => return content.child(text(format!("无法读取成就目录：{error}"))),
        };
        let records = match store::records(&database, &uid) {
            Ok(records) => records,
            Err(error) => return content.child(text(error)),
        };
        let database_for_export = database.clone();
        let uid_for_export = uid.clone();
        let message_uid = uid.clone();
        let save_database = database.clone();
        let save_uid = uid.clone();
        let save = move |id, completed| match store::set_completion(
            &save_database,
            &save_uid,
            id,
            completed,
        ) {
            Ok(()) => {
                message.set(None);
                revision.update(|value| *value += 1);
            }
            Err(error) => message.set(Some((save_uid.clone(), format!("保存成就失败：{error}")))),
        };
        let has_records = !records.is_empty();
        content
            .child(text(format!("成就目录 · 原神 {}", catalog.version)))
            .child((!has_records).then(|| text("暂无成就进度")))
            .child(has_records.then(|| {
                button("导出 UIAF v1.1", move || {
                    let folder = database_for_export.with_file_name("achievement-exports");
                    let result = match store::export(&database_for_export, &uid_for_export, &folder)
                    {
                        Ok(path) => format!(
                            "UID {uid_for_export} 的 UIAF v1.1 已导出：{}",
                            path.display()
                        ),
                        Err(error) => format!("导出失败：{error}"),
                    };
                    message.set(Some((uid_for_export.clone(), result)));
                })
            }))
            .child(dynamic(move || {
                message
                    .get()
                    .filter(|(uid, _)| *uid == message_uid)
                    .map(|(_, value)| text(value))
            }))
            .child(
                input()
                    .test_id("achievement-search")
                    .label("按成就名称或 ID 搜索")
                    .placeholder("按成就名称或 ID 搜索")
                    .h_10()
                    .px_3()
                    .bg(move || theme().secondary)
                    .value(move || query.get())
                    .on_change(move |value: &str| {
                        query.set(value.to_owned());
                        position.set((String::new(), 0));
                    }),
            )
            .child(dynamic(move || {
                let (shown_uid, index) = position.get();
                let index = if shown_uid == uid { index } else { 0 };
                let uid = uid.clone();
                results(
                    catalog,
                    &records,
                    query.get().trim(),
                    index,
                    move |page| position.set((uid.clone(), page)),
                    save.clone(),
                )
            }))
    })
}

fn results(
    catalog: &Catalog,
    records: &[Record],
    query: &str,
    index: usize,
    navigate: impl Fn(usize) + Clone + 'static,
    save: impl Fn(u32, bool) + Clone + 'static,
) -> View {
    let mut saved: BTreeMap<_, _> = records.iter().map(|record| (record.id, *record)).collect();
    let known = catalog
        .entries
        .iter()
        .map(|entry| (entry.id, Some(entry), saved.remove(&entry.id)));
    let mut rows: Vec<_> = known.collect();
    rows.extend(
        saved
            .into_iter()
            .map(|(id, record)| (id, None, Some(record))),
    );
    let query_id = query.parse::<u32>().ok();
    rows.retain(|(id, entry, _)| {
        query.is_empty()
            || query_id == Some(*id)
            || entry.is_some_and(|entry| entry.title.contains(query))
    });

    let content = view().gap_2();
    if rows.is_empty() {
        return content.child(text("未找到匹配的成就"));
    }
    let completed = rows
        .iter()
        .filter(|(_, _, record)| record.is_some_and(Record::completed))
        .count();
    let page_count = rows.len().div_ceil(PAGE_SIZE);
    let index = index.min(page_count - 1);
    let previous = navigate.clone();
    content
        .child(text(format!(
            "已完成 {completed} 项 · 共 {} 项 · 第 {}/{page_count} 页",
            rows.len(),
            index + 1
        )))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child((index > 0).then(|| button("上一页", move || previous(index - 1))))
                .child(
                    (index + 1 < page_count)
                        .then(|| button("下一页", move || navigate(index + 1))),
                ),
        )
        .children(
            rows.into_iter()
                .skip(index * PAGE_SIZE)
                .take(PAGE_SIZE)
                .map(|(id, entry, record)| {
                    let save = save.clone();
                    let completed = record.is_some_and(Record::completed);
                    let label = record.map_or_else(|| format!("成就 {id} · 未完成"), Record::label);
                    let (label, name) = match entry {
                        Some(entry) => {
                            let title = &entry.title;
                            let label = entry.stage.as_ref().map_or_else(
                                || format!("{title} · {label}"),
                                |stage| {
                                    format!(
                                        "{title} · 第 {} 阶段 · 目标 {} · {label}",
                                        stage.number, stage.target
                                    )
                                },
                            );
                            (label, format!("{title}（{id}）"))
                        }
                        None => (label, format!("成就 {id}")),
                    };
                    let action = if completed {
                        "取消完成"
                    } else {
                        "标记完成"
                    };
                    let mut row = view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(text(label).min_w(px(0.0)))
                        .child(button(format!("{action}：{name}"), move || {
                            save(id, !completed);
                        }));
                    if id == 80_001 {
                        row = row.test_id("achievement-row-80001");
                    }
                    row.into_element()
                }),
        )
}
