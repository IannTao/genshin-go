//! Windows client for synchronizing material counts via the `MiYouShe` calculator API.
use crate::{account_store, profile::VerifiedAccount};
use reqwest::{Client, StatusCode, header, redirect::Policy};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::OnceLock,
    time::Duration,
};

const AVATAR_LIST_URL: &str =
    "https://api-takumi.mihoyo.com/event/e20200928calculate/v1/avatar/list";
const WEAPON_LIST_URL: &str =
    "https://api-takumi.mihoyo.com/event/e20200928calculate/v1/weapon/list";
const BATCH_COMPUTE_URL: &str =
    "https://api-takumi.mihoyo.com/event/e20200928calculate/v3/batch_compute";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const REFERER: &str = "https://webstatic.mihoyo.com";
const OFFICIAL_REGION: &str = "cn_gf01";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REPLY_BYTES: usize = 12 * 1024 * 1024;
const TRAVELER_IDS: [i64; 2] = [10_000_005, 10_000_007];

#[derive(Deserialize)]
struct AvatarListData {
    list: Vec<AvatarEntry>,
}

#[derive(Deserialize)]
struct AvatarEntry {
    id: i64,
    #[serde(default)]
    name: String,
    weapon_cat_id: i64,
    #[serde(default)]
    skill_list: Vec<SkillEntry>,
}

#[derive(Deserialize)]
struct SkillEntry {
    group_id: i64,
    #[serde(default)]
    max_level: u8,
}

#[derive(Deserialize)]
struct WeaponListData {
    list: Vec<WeaponEntry>,
}

#[derive(Deserialize)]
struct WeaponEntry {
    id: i64,
    weapon_cat_id: i64,
    #[serde(default)]
    weapon_level: u8,
    #[serde(default)]
    max_level: u8,
}

#[derive(Deserialize)]
struct ComputeData {
    #[serde(default)]
    has_user_info: bool,
    #[serde(default)]
    overall_consume: Vec<ConsumeEntry>,
    #[serde(default)]
    available_material: Vec<AvailableEntry>,
}

#[derive(Deserialize)]
struct ConsumeEntry {
    id: i32,
    num: u32,
    lack_num: u32,
}

#[derive(Deserialize)]
struct AvailableEntry {
    id: i32,
    num: u32,
}

/// Holds parsed material counts returned by the `MiYouShe` calculator.
struct SyncedMaterials {
    good_materials: BTreeMap<String, u32>,
    #[cfg(test)]
    unmapped_count: usize,
}

fn good_key_map() -> Result<&'static BTreeMap<i32, String>, String> {
    static KEYS: OnceLock<Result<BTreeMap<i32, String>, serde_json::Error>> = OnceLock::new();
    KEYS.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/growth-material-good-keys.json"))
    })
    .as_ref()
    .map_err(|error| format!("无法读取 GOOD 材料目录：{error}"))
}

fn build_client() -> Result<Client, String> {
    Client::builder()
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|_| "无法初始化米游社材料同步客户端".into())
}

async fn post_json(
    client: &Client,
    url: &str,
    cookie: &str,
    body: &Value,
) -> Result<Value, String> {
    let mut cookie_value =
        header::HeaderValue::from_str(cookie).map_err(|_| "米游社授权格式无效，请重新扫码")?;
    cookie_value.set_sensitive(true);
    let request_bytes =
        serde_json::to_vec(body).map_err(|_| "无法构造米游社养成计算器请求".to_owned())?;
    let mut reply = client
        .post(url)
        .header(header::COOKIE, cookie_value)
        .header(header::REFERER, REFERER)
        .header(header::CONTENT_TYPE, "application/json")
        .body(request_bytes)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                "连接米游社养成计算器超时，请重试".to_owned()
            } else {
                "网络失败：无法连接米游社养成计算器".to_owned()
            }
        })?;
    let status = reply.status();
    match status {
        StatusCode::UNAUTHORIZED => return Err("米游社授权已失效，请重新扫码".into()),
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => {
            return Err("米游社养成计算器响应超时，请重试".into());
        }
        status if !status.is_success() => {
            return Err(format!(
                "米游社养成计算器请求失败（HTTP {}）",
                status.as_u16()
            ));
        }
        _ => {}
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = reply
        .chunk()
        .await
        .map_err(|_| "读取米游社养成计算器响应失败".to_owned())?
    {
        if bytes.len() + chunk.len() > MAX_REPLY_BYTES {
            return Err("米游社养成计算器响应过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let payload: Value =
        serde_json::from_slice(&bytes).map_err(|_| "米游社养成计算器响应格式异常")?;
    let code = payload["retcode"]
        .as_i64()
        .ok_or("米游社养成计算器响应缺少状态码")?;
    let message = payload["message"].as_str().unwrap_or("");
    let verification = message.contains("验证码") || message.contains("人机验证");
    match code {
        0 => Ok(payload["data"].clone()),
        -100 => Err("米游社授权已失效，请重新扫码".into()),
        1034 => Err("米游社要求安全验证，请在米游社完成验证后重试".into()),
        n if verification => Err(format!("米游社要求安全验证（服务代码 {n}），请稍后重试")),
        n => Err(format!("米游社养成计算器拒绝请求（服务代码 {n}）")),
    }
}

fn build_compute_items(avatars: &[AvatarEntry], weapons: &[WeaponEntry]) -> Vec<Value> {
    let mut by_category: HashMap<i64, Vec<&WeaponEntry>> = HashMap::new();
    for weapon in weapons {
        by_category
            .entry(weapon.weapon_cat_id)
            .or_default()
            .push(weapon);
    }
    let mut cursor: HashMap<i64, usize> = HashMap::new();
    let mut items = Vec::new();
    for avatar in avatars {
        if avatar.name == "旅行者" || TRAVELER_IDS.contains(&avatar.id) {
            continue;
        }
        let skills: Vec<Value> = avatar
            .skill_list
            .iter()
            .filter(|skill| skill.max_level > 1)
            .map(|skill| {
                json!({
                    "id": skill.group_id,
                    "level_current": 1,
                    "level_target": skill.max_level.min(10),
                })
            })
            .collect();
        if skills.is_empty() {
            continue;
        }
        let mut entry = json!({
            "avatar_id": avatar.id,
            "avatar_level_current": 1,
            "avatar_level_target": 90,
            "skill_list": skills,
        });
        if let Some(pool) = by_category.get(&avatar.weapon_cat_id)
            && !pool.is_empty()
        {
            let index = cursor.entry(avatar.weapon_cat_id).or_insert(0);
            let weapon = pool[*index % pool.len()];
            *index += 1;
            let target = if weapon.max_level > 0 {
                weapon.max_level
            } else if weapon.weapon_level <= 2 {
                70
            } else {
                90
            };
            entry["weapon"] = json!({
                "id": weapon.id,
                "level_current": 1,
                "level_target": target,
            });
        }
        items.push(entry);
    }
    items
}

fn parse_compute_materials(data: &ComputeData) -> Result<SyncedMaterials, String> {
    if !data.has_user_info {
        return Err("米游社未返回当前 UID 的背包材料数据，请检查米游社角色绑定后重试".into());
    }
    let keys = good_key_map()?;
    let mut raw_owned: BTreeMap<i32, u32> = BTreeMap::new();
    for item in &data.overall_consume {
        let owned = item.num.saturating_sub(item.lack_num);
        if owned > 0 {
            raw_owned
                .entry(item.id)
                .and_modify(|current| *current = (*current).max(owned))
                .or_insert(owned);
        }
    }
    for item in &data.available_material {
        if item.num > 0 {
            raw_owned.insert(item.id, item.num);
        } else {
            raw_owned.remove(&item.id);
        }
    }
    let mut good_materials = BTreeMap::new();
    #[cfg(test)]
    let mut unmapped_count = 0;
    for (id, count) in raw_owned {
        if let Some(good_key) = keys.get(&id) {
            good_materials.insert(good_key.clone(), count);
        } else {
            #[cfg(test)]
            {
                unmapped_count += 1;
            }
        }
    }
    Ok(SyncedMaterials {
        good_materials,
        #[cfg(test)]
        unmapped_count,
    })
}

async fn fetch_compute_data(
    client: &Client,
    cookie: &str,
    uid: &str,
) -> Result<ComputeData, String> {
    let avatar_data: AvatarListData = serde_json::from_value(
        post_json(
            client,
            AVATAR_LIST_URL,
            cookie,
            &json!({ "page": 1, "size": 1000, "is_all": true }),
        )
        .await?,
    )
    .map_err(|_| "米游社角色列表格式异常")?;
    let weapon_data: WeaponListData = serde_json::from_value(
        post_json(
            client,
            WEAPON_LIST_URL,
            cookie,
            &json!({ "page": 1, "size": 1000, "weapon_levels": [1, 2, 3, 4, 5] }),
        )
        .await?,
    )
    .map_err(|_| "米游社武器列表格式异常")?;
    let items = build_compute_items(&avatar_data.list, &weapon_data.list);
    if items.is_empty() {
        return Err("未从米游社获取到可计算的养成目录".into());
    }
    serde_json::from_value(
        post_json(
            client,
            BATCH_COMPUTE_URL,
            cookie,
            &json!({
                "items": items,
                "region": OFFICIAL_REGION,
                "uid": uid,
            }),
        )
        .await?,
    )
    .map_err(|_| "米游社养成计算结果格式异常".into())
}

/// Fetches owned growth materials for the verified account's official CN UID.
pub(super) async fn fetch_materials(
    database: PathBuf,
    account: VerifiedAccount,
) -> Result<BTreeMap<String, u32>, String> {
    let expected = account.clone();
    let grant = tokio::task::spawn_blocking(move || {
        account_store::load_community_grant(&database, &expected)
    })
    .await
    .map_err(|_| "无法读取本机米游社授权".to_owned())??
    .ok_or("米游社授权已变化或移除，请先重新检查授权")?;
    let client = build_client()?;
    let compute_data = fetch_compute_data(&client, &grant.web_cookie, &account.uid).await?;
    Ok(parse_compute_materials(&compute_data)?.good_materials)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_compute_items_skipping_traveler_and_rotating_weapons() {
        let avatars = vec![
            AvatarEntry {
                id: 10_000_005,
                name: "旅行者".into(),
                weapon_cat_id: 1,
                skill_list: vec![SkillEntry {
                    group_id: 1,
                    max_level: 10,
                }],
            },
            AvatarEntry {
                id: 10_000_021,
                name: "安柏".into(),
                weapon_cat_id: 12,
                skill_list: vec![
                    SkillEntry {
                        group_id: 211,
                        max_level: 10,
                    },
                    SkillEntry {
                        group_id: 214,
                        max_level: 1,
                    },
                ],
            },
            AvatarEntry {
                id: 10_000_022,
                name: "温迪".into(),
                weapon_cat_id: 12,
                skill_list: vec![SkillEntry {
                    group_id: 221,
                    max_level: 10,
                }],
            },
        ];
        let weapons = vec![
            WeaponEntry {
                id: 15_101,
                weapon_cat_id: 12,
                weapon_level: 1,
                max_level: 70,
            },
            WeaponEntry {
                id: 15_501,
                weapon_cat_id: 12,
                weapon_level: 5,
                max_level: 90,
            },
        ];
        let items = build_compute_items(&avatars, &weapons);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["avatar_id"], 10_000_021);
        assert_eq!(items[0]["skill_list"].as_array().map(Vec::len), Some(1));
        assert_eq!(items[0]["weapon"]["id"], 15_101);
        assert_eq!(items[0]["weapon"]["level_target"], 70);
        assert_eq!(items[1]["weapon"]["id"], 15_501);
        assert_eq!(items[1]["weapon"]["level_target"], 90);
    }

    #[test]
    fn maps_calculator_materials_to_good_keys_and_rejects_missing_user_info() -> Result<(), String>
    {
        let unauthorized = ComputeData {
            has_user_info: false,
            overall_consume: vec![],
            available_material: vec![],
        };
        assert!(parse_compute_materials(&unauthorized).is_err());

        let data = ComputeData {
            has_user_info: true,
            overall_consume: vec![
                ConsumeEntry {
                    id: 202,
                    num: 1_000_000,
                    lack_num: 200_000,
                },
                ConsumeEntry {
                    id: 100_055,
                    num: 168,
                    lack_num: 168,
                },
                ConsumeEntry {
                    id: 104_111,
                    num: 18,
                    lack_num: 0,
                },
                ConsumeEntry {
                    id: 104_122,
                    num: 153,
                    lack_num: 0,
                },
                ConsumeEntry {
                    id: 999_999,
                    num: 10,
                    lack_num: 2,
                },
            ],
            available_material: vec![
                AvailableEntry {
                    id: 100_021,
                    num: 42,
                },
                AvailableEntry {
                    id: 104_111,
                    num: 375,
                },
                AvailableEntry {
                    id: 104_122,
                    num: 0,
                },
            ],
        };
        let synced = parse_compute_materials(&data)?;
        assert_eq!(synced.good_materials.get("Mora"), Some(&800_000));
        assert_eq!(synced.good_materials.get("Wolfhook"), Some(&42));
        assert_eq!(synced.good_materials.get("AgnidusAgateSliver"), Some(&375));
        assert!(
            !synced
                .good_materials
                .contains_key("VarunadaLazuriteFragment")
        );
        assert!(!synced.good_materials.contains_key("SmallLampGrass"));
        assert_eq!(synced.unmapped_count, 1);
        Ok(())
    }

    struct TempFile(PathBuf);

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    #[ignore = "probes the live MiYouShe calculator API using the local saved authorization"]
    fn live_probe_mihoyo_materials() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA missing")?;
        let source_db = PathBuf::from(root)
            .join("Genshin-Go")
            .join("launcher.sqlite3");
        let db = TempFile(std::env::temp_dir().join(format!(
            "genshin-go-mihoyo-probe-{}.sqlite3",
            std::process::id()
        )));
        std::fs::copy(&source_db, &db.0)?;
        let profile = account_store::load(&db.0)?;
        let account = profile
            .account
            .ok_or("No saved MiYouShe authorization in local launcher.sqlite3")?;
        let grant = account_store::load_community_grant(&db.0, &account)?
            .ok_or("Failed to load community grant")?;
        drop(db);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let client = build_client()?;
            let compute_data = fetch_compute_data(&client, &grant.web_cookie, &account.uid).await?;
            let synced = parse_compute_materials(&compute_data)?;
            eprintln!(
                "[probe] uid={} has_user_info={} overall_consume={} available_material={} mapped_good={} unmapped={}",
                account.uid,
                compute_data.has_user_info,
                compute_data.overall_consume.len(),
                compute_data.available_material.len(),
                synced.good_materials.len(),
                synced.unmapped_count
            );
            Ok::<_, String>(())
        })?;
        Ok(())
    }
}
