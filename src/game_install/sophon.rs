use futures_util::{StreamExt, stream};
use prost::Message;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{io::Read, time::Duration};

const BRANCHES: &str = "https://hyp-api.mihoyo.com/hyp/hyp-connect/api/getGameBranches?launcher_id=jGHBHlcOq1&game_ids%5B%5D=1Z8W5NHUQb";
const BUILD: &str = "https://api-takumi.mihoyo.com/downloader/sophon_chunk/api/getBuild";
const MANIFEST_LIMIT: u64 = 64 * 1024 * 1024;
const CHUNK_LIMIT: u64 = 16 * 1024 * 1024;
const CONCURRENCY: usize = 12;

#[derive(Deserialize)]
struct Response<T> {
    retcode: i32,
    message: String,
    data: Option<T>,
}

#[derive(Deserialize)]
struct Branches {
    game_branches: Vec<Game>,
}

#[derive(Deserialize)]
struct Game {
    game: GameId,
    main: Branch,
}

#[derive(Deserialize)]
struct GameId {
    id: String,
    biz: String,
}

#[derive(Deserialize)]
struct Branch {
    #[serde(rename = "branch")]
    name: String,
    package_id: String,
    password: String,
    tag: String,
}

#[derive(Deserialize, Serialize)]
/// Identifies the official build and its selected resource categories.
pub(super) struct Build {
    pub(super) tag: String,
    #[serde(rename = "build_id")]
    id: String,
    pub(super) manifests: Vec<Category>,
}

#[derive(Deserialize, Serialize)]
/// Describes the manifest and download locations for one resource category.
pub(super) struct Category {
    pub(super) matching_field: String,
    manifest: ManifestId,
    manifest_download: Download,
    chunk_download: Download,
}

#[derive(Deserialize, Serialize)]
struct Download {
    url_prefix: String,
    url_suffix: String,
    compression: u32,
    encryption: u32,
}

impl Download {
    fn url(&self, id: &str) -> Result<Url, String> {
        let valid_id = !id.is_empty()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        if !valid_id || self.compression != 1 || self.encryption != 0 {
            return Err("官方资源使用了未知的 Sophon 下载格式".into());
        }
        let url = Url::parse(&format!("{}/{id}{}", self.url_prefix, self.url_suffix))
            .map_err(|error| error.to_string())?;
        if url.scheme() != "https"
            || url.host_str() != Some("autopatchcn.yuanshen.com")
            || !url.path().starts_with("/client_app/sophon/")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
        {
            return Err("官方资源地址不在国服 Sophon CDN 上".into());
        }
        Ok(url)
    }
}

#[derive(Deserialize, Serialize)]
struct ManifestId {
    id: String,
    checksum: String,
    compressed_size: String,
    uncompressed_size: String,
}

#[derive(Message)]
/// Lists the assets in one Sophon full-install manifest.
pub(super) struct Manifest {
    #[prost(message, repeated, tag = "1")]
    pub(super) assets: Vec<Asset>,
}

#[derive(Message)]
/// Describes the official path and complete contents of an asset.
pub(super) struct Asset {
    #[prost(string, tag = "1")]
    pub(super) path: String,
    #[prost(message, repeated, tag = "2")]
    pub(super) chunks: Vec<Chunk>,
    #[prost(bool, tag = "3")]
    pub(super) directory: bool,
    #[prost(uint64, tag = "4")]
    pub(super) size: u64,
    #[prost(string, tag = "5")]
    pub(super) md5: String,
}

#[derive(Message)]
/// Describes a compressed chunk and its position in an asset.
pub(super) struct Chunk {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, tag = "2")]
    pub(super) md5: String,
    #[prost(uint64, tag = "3")]
    pub(super) offset: u64,
    #[prost(uint64, tag = "4")]
    compressed_size: u64,
    #[prost(uint64, tag = "5")]
    pub(super) size: u64,
}

/// Creates the HTTP client used for the official Sophon distribution.
pub(super) fn client() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|error| error.to_string())
}

async fn get(client: &Client, url: &Url, limit: u64) -> Result<Vec<u8>, String> {
    for delay in [Some(500), Some(1500), None] {
        match get_once(client, url, limit).await {
            Ok(Some(bytes)) => return Ok(bytes),
            Ok(None) => return Err("下载内容超出清单声明的大小".into()),
            Err(error) => {
                let retryable = error.is_timeout()
                    || error.is_connect()
                    || error.is_body()
                    || error
                        .status()
                        .is_some_and(|status| status.as_u16() == 429 || status.is_server_error());
                if let Some(delay) = delay.filter(|_| retryable) {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                } else {
                    return Err(error.without_url().to_string());
                }
            }
        }
    }
    Err("无法下载官方资源".into())
}

async fn get_once(
    client: &Client,
    url: &Url,
    limit: u64,
) -> Result<Option<Vec<u8>>, reqwest::Error> {
    let mut response = client.get(url.clone()).send().await?.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() as u64 + chunk.len() as u64 > limit {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Some(bytes))
}

async fn api<T: DeserializeOwned>(client: &Client, url: &Url) -> Result<T, String> {
    let bytes = get(client, url, 2 * 1024 * 1024).await?;
    let response: Response<T> =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if response.retcode != 0 {
        return Err(format!(
            "官方清单服务返回 {}：{}",
            response.retcode, response.message
        ));
    }
    response
        .data
        .ok_or_else(|| "官方清单服务没有返回数据".into())
}

/// Resolves the current mainland-China production build.
pub(super) async fn latest(client: &Client) -> Result<Build, String> {
    let url = Url::parse(BRANCHES).map_err(|error| error.to_string())?;
    let branches: Branches = api(client, &url).await?;
    let branch = branches
        .game_branches
        .into_iter()
        .find(|game| game.game.id == "1Z8W5NHUQb" && game.game.biz == "hk4e_cn")
        .ok_or("官方清单未返回国服官服分支")?
        .main;
    if branch.name != "main" {
        return Err("官方清单未返回国服官服正式分支".into());
    }

    let url = Url::parse_with_params(
        BUILD,
        [
            ("package_id", branch.package_id.as_str()),
            ("branch", branch.name.as_str()),
            ("password", branch.password.as_str()),
            ("tag", branch.tag.as_str()),
        ],
    )
    .map_err(|error| error.to_string())?;
    let mut build: Build = api(client, &url).await?;
    if build.tag != branch.tag {
        return Err("国服官服分支与构建的版本不一致".into());
    }
    build
        .manifests
        .retain(|category| matches!(category.matching_field.as_str(), "game" | "zh-cn"));
    build.validate()?;
    Ok(build)
}

impl Build {
    /// Checks the saved build's version and required categories.
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.tag.split('.').count() != 3
            || self.tag.split('.').any(|part| part.parse::<u32>().is_err())
            || self.manifests.len() != 2
            || !self
                .manifests
                .iter()
                .any(|category| category.matching_field == "game")
            || !self
                .manifests
                .iter()
                .any(|category| category.matching_field == "zh-cn")
        {
            return Err("官方构建缺少可用的版本、游戏资源或中文语音".into());
        }
        Ok(())
    }
}

impl Category {
    /// Fetches this category's compressed file manifest.
    pub(super) async fn fetch_manifest(&self, client: &Client) -> Result<Vec<u8>, String> {
        let url = self.manifest_download.url(&self.manifest.id)?;
        let limit = declared_size(&self.manifest.compressed_size, MANIFEST_LIMIT)?;
        get(client, &url, limit).await
    }

    /// Verifies and decodes a category's compressed file manifest.
    pub(super) fn decode_manifest(&self, compressed: &[u8]) -> Result<Manifest, String> {
        let compressed_size = declared_size(&self.manifest.compressed_size, MANIFEST_LIMIT)?;
        let size = declared_size(&self.manifest.uncompressed_size, MANIFEST_LIMIT)?;
        let bytes = unpack(compressed, compressed_size, size, &self.manifest.checksum)?;
        Manifest::decode(bytes.as_slice()).map_err(|error| error.to_string())
    }

    /// Downloads verified chunks in parallel and yields them in file order.
    pub(super) fn download_chunks<'a>(
        &'a self,
        client: &'a Client,
        chunks: &'a [Chunk],
    ) -> impl futures_util::Stream<Item = Result<Vec<u8>, String>> + 'a {
        stream::iter(chunks)
            .map(move |chunk| async move {
                let url = self.chunk_download.url(&chunk.id)?;
                let compressed = get(client, &url, chunk.compressed_size).await?;
                unpack(&compressed, chunk.compressed_size, chunk.size, &chunk.md5)
                    .map_err(|error| format!("资源分块 {}：{error}", chunk.id))
            })
            .buffered(CONCURRENCY)
    }
}

impl Asset {
    /// Orders the chunks and validates that they exactly cover the file.
    pub(super) fn validate_chunks(&mut self) -> Result<(), String> {
        self.chunks.sort_unstable_by_key(|chunk| chunk.offset);
        let mut end = 0;
        for chunk in &self.chunks {
            if chunk.offset != end
                || !(1..=CHUNK_LIMIT).contains(&chunk.size)
                || !(1..=CHUNK_LIMIT).contains(&chunk.compressed_size)
                || !valid_md5(&chunk.md5)
            {
                return Err(format!("官方分块范围无效：{}", self.path));
            }
            end = end.checked_add(chunk.size).ok_or("官方分块大小溢出")?;
        }
        if self.size != end || (!self.directory && !valid_md5(&self.md5)) {
            return Err(format!("官方文件清单无效：{}", self.path));
        }
        Ok(())
    }
}

fn valid_md5(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn declared_size(value: &str, limit: u64) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|size| (1..=limit).contains(size))
        .ok_or_else(|| "官方清单的大小超出支持范围".into())
}

fn unpack(
    compressed: &[u8],
    compressed_size: u64,
    size: u64,
    checksum: &str,
) -> Result<Vec<u8>, String> {
    if compressed.len() as u64 != compressed_size {
        return Err("下载内容长度与清单不符".into());
    }
    let decoder =
        zstd::stream::read::Decoder::new(compressed).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    decoder
        .take(size + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 != size
        || !format!("{:x}", md5::compute(&bytes)).eq_ignore_ascii_case(checksum)
    {
        return Err("解压后内容长度或 MD5 与清单不符".into());
    }
    Ok(bytes)
}
