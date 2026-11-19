use super::{DIRECTORY_NAME, InstallControl};
use crate::native_environment::{atomic_write, validate_installation};
use futures_util::{StreamExt, pin_mut};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[path = "sophon.rs"]
mod sophon;

const JOURNAL: &str = "game-install.json";
const MARKER: &str = ".genshin-go-install-id";

#[derive(Deserialize, Serialize)]
struct Job {
    id: String,
    parent: PathBuf,
    build: Option<sophon::Build>,
}

impl Job {
    fn load(journal: &Path) -> Result<Option<Self>, String> {
        let Some(bytes) = optional(fs::read(journal))? else {
            return Ok(None);
        };
        let job: Self = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if !job.parent.is_absolute()
            || job.id.is_empty()
            || !job.id.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("本机安装任务记录无效".into());
        }
        if let Some(build) = &job.build {
            build.validate()?;
        }
        Ok(Some(job))
    }

    fn save(&self, journal: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        atomic_write(journal, &bytes)
    }

    fn new(parent: &Path) -> Result<Self, String> {
        let parent = crate::native_environment::normalize_directory(parent)?;
        if !parent.is_dir()
            || parent.join("YuanShen.exe").exists()
            || validate_installation(&parent).is_ok()
        {
            return Err("请选择存放新安装的上一级目录".into());
        }
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
            .to_string();
        let job = Self {
            id,
            parent,
            build: None,
        };
        require_absent(&job.destination())?;
        require_absent(&job.staging())?;
        Ok(job)
    }

    fn destination(&self) -> PathBuf {
        self.parent.join(DIRECTORY_NAME)
    }

    fn staging(&self) -> PathBuf {
        self.parent.join(format!(".genshin-go-install-{}", self.id))
    }

    fn owns_destination(&self, destination: &Path) -> Result<bool, String> {
        let Some(metadata) = optional(fs::symlink_metadata(destination))? else {
            return Ok(false);
        };
        if metadata.is_dir() && !is_reparse_point(&metadata) {
            let marker = destination.join(MARKER);
            if regular_file(&marker)?.is_some()
                && fs::read_to_string(marker).map_err(io_error)? == self.id
            {
                return Ok(true);
            }
        }
        Err(format!(
            "安装目录已存在：{}。请选择没有同名子目录的位置。",
            destination.display()
        ))
    }
}

/// Returns the saved destination without starting any download.
pub(crate) fn pending(settings: &Path) -> Result<Option<PathBuf>, String> {
    Ok(Job::load(&settings.with_file_name(JOURNAL))?.map(|job| job.destination()))
}

/// Creates or resumes a Windows installation and publishes verified files.
pub(crate) fn install(
    settings: &Path,
    parent: Option<&Path>,
    control: &InstallControl,
    set_default: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Option<PathBuf>, String> {
    let journal = settings.with_file_name(JOURNAL);
    if let Some(directory) = journal.parent() {
        fs::create_dir_all(directory).map_err(io_error)?;
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(journal.with_extension("lock"))
        .map_err(io_error)?;
    lock.try_lock()
        .map_err(|error| format!("无法锁定本机安装任务，请在另一启动器停止下载后重试：{error}"))?;

    let mut job = match (parent, Job::load(&journal)?) {
        (Some(parent), None) => {
            let job = Job::new(parent)?;
            let stage = job.staging();
            fs::create_dir(&stage)
                .map_err(|error| format!("无法写入安装位置 {}：{error}", job.parent.display()))?;
            if let Err(error) = job.save(&journal) {
                let _ = fs::remove_dir(stage);
                return Err(error);
            }
            job
        }
        (None, Some(job)) => job,
        (None, None) => return Err("没有可继续的安装任务".into()),
        (Some(_), Some(_)) => return Err("已有未完成的安装任务，请先继续该任务".into()),
    };
    let destination = job.destination();
    let stage = job.staging();
    if crate::native_environment::normalize_directory(&job.parent)? != job.parent {
        return Err("安装位置已变化，请恢复原目录后继续".into());
    }

    if !job.owns_destination(&destination)? {
        if control.is_paused() {
            return Ok(None);
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(io_error)?;
        let completed = runtime.block_on(download(&journal, &mut job, control))?;
        if !completed || control.is_paused() {
            return Ok(None);
        }
        require_absent(&destination)?;
        fs::rename(stage.join("game"), &destination).map_err(io_error)?;
    }

    let directory = validate_installation(&destination)?;
    set_default(&directory)?;
    fs::remove_file(journal).map_err(io_error)?;
    let _ = fs::remove_file(destination.join(MARKER));
    if fs::symlink_metadata(&stage)
        .is_ok_and(|metadata| metadata.is_dir() && !is_reparse_point(&metadata))
    {
        for cache in ["game.pb.zst", "zh-cn.pb.zst"] {
            let _ = fs::remove_file(stage.join(cache));
        }
        let _ = fs::remove_dir(stage.join("parts"));
        let _ = fs::remove_dir(stage);
    }
    Ok(Some(directory))
}

async fn download(journal: &Path, job: &mut Job, control: &InstallControl) -> Result<bool, String> {
    let client = sophon::client()?;
    if job.build.is_none() {
        let build = tokio::select! {
            result = sophon::latest(&client) => result?,
            () = wait_for_pause(control) => return Ok(false),
        };
        job.build = Some(build);
        job.save(journal)?;
    }
    let build = job.build.as_ref().ok_or("安装任务未绑定官方构建")?;
    let stage = job.staging();
    let game = stage.join("game");
    ensure_directory(&stage)?;
    ensure_directory(&game)?;

    let mut resources = Vec::new();
    let mut files = BTreeMap::new();
    let mut total = 0_u64;
    for category in &build.manifests {
        if control.is_paused() {
            return Ok(false);
        }
        let cache = stage.join(format!("{}.pb.zst", category.matching_field));
        let mut manifest = tokio::select! {
            result = load_manifest(&client, category, &cache) => result?,
            () = wait_for_pause(control) => return Ok(false),
        };
        for asset in &mut manifest.assets {
            validate_path(&asset.path)?;
            asset.validate_chunks()?;
            let key = asset.path.replace('\\', "/").to_lowercase();
            if let Some(previous) = files.insert(key, asset.directory)
                && !(previous && asset.directory)
            {
                return Err(format!("官方清单含有重复路径：{}", asset.path));
            }
            total = total.checked_add(asset.size).ok_or("官方安装大小溢出")?;
        }
        resources.push((category, manifest));
    }

    let mut written = 0;
    control.report(written, total);
    for (category, manifest) in &resources {
        for asset in &manifest.assets {
            if control.is_paused() {
                return Ok(false);
            }
            let path = game.join(&asset.path);
            let parent = path.parent().ok_or("官方文件缺少父目录")?;
            create_subdirectories(&game, parent)?;
            if asset.directory {
                ensure_directory(&path)?;
            } else if let Some(metadata) = regular_file(&path)? {
                if metadata.len() != asset.size {
                    return Err(format!("已校验文件大小发生变化：{}", asset.path));
                }
                written += asset.size;
                control.report(written, total);
            } else {
                let transfer = Transfer {
                    client: &client,
                    category,
                    control,
                    total,
                };
                if !transfer.file(&stage, asset, &path, &mut written).await? {
                    return Ok(false);
                }
            }
        }
    }

    let config = format!(
        "[General]\r\nchannel=1\r\nsub_channel=1\r\ngame_biz=hk4e_cn\r\ngame_version={}\r\n",
        build.tag
    );
    atomic_write(&game.join("config.ini"), config.as_bytes())?;
    let persistent = game.join("YuanShen_Data/Persistent");
    create_subdirectories(&game, &persistent)?;
    atomic_write(&persistent.join("audio_lang_14"), b"Chinese\r\n")?;
    validate_installation(&game)?;
    atomic_write(&game.join(MARKER), job.id.as_bytes())?;
    Ok(true)
}

async fn load_manifest(
    client: &Client,
    category: &sophon::Category,
    cache: &Path,
) -> Result<sophon::Manifest, String> {
    if let Some(metadata) = regular_file(cache)?
        && metadata.len() <= 64 * 1024 * 1024
        && let Ok(manifest) = category.decode_manifest(&fs::read(cache).map_err(io_error)?)
    {
        return Ok(manifest);
    }
    let bytes = category.fetch_manifest(client).await?;
    let manifest = category.decode_manifest(&bytes)?;
    atomic_write(cache, &bytes)?;
    Ok(manifest)
}

struct Transfer<'a> {
    client: &'a Client,
    category: &'a sophon::Category,
    control: &'a InstallControl,
    total: u64,
}

impl Transfer<'_> {
    async fn file(
        &self,
        stage: &Path,
        asset: &sophon::Asset,
        path: &Path,
        written: &mut u64,
    ) -> Result<bool, String> {
        // A dedicated file per asset keeps progress even if the process ends
        // between publishing a completed file and starting the next one.
        let parts = stage.join("parts");
        ensure_directory(&parts)?;
        let partial = parts.join(format!("{:x}.part", md5::compute(asset.path.as_bytes())));
        regular_file(&partial)?;
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .truncate(false)
            .open(&partial)
            .map_err(io_error)?;
        let Some((finished, length, mut digest)) =
            self.verified_prefix(&mut file, &asset.chunks)?
        else {
            return Ok(false);
        };
        file.set_len(length)
            .and_then(|()| file.seek(SeekFrom::Start(length)))
            .map_err(io_error)?;
        *written += length;
        self.control.report(*written, self.total);

        let pending = self
            .category
            .download_chunks(self.client, &asset.chunks[finished..]);
        pin_mut!(pending);
        while !self.control.is_paused() {
            let outcome = tokio::select! {
                result = pending.next() => result,
                () = wait_for_pause(self.control) => break,
            };
            let Some(bytes) = outcome else { break };
            let result = bytes.and_then(|bytes| {
                file.write_all(&bytes).map_err(io_error)?;
                digest.consume(&bytes);
                *written += bytes.len() as u64;
                self.control.report(*written, self.total);
                Ok(())
            });
            if let Err(error) = result {
                file.sync_all().map_err(io_error)?;
                return Err(format!("{}：{error}", asset.path));
            }
        }
        file.sync_all().map_err(io_error)?;
        drop(file);
        if self.control.is_paused() {
            return Ok(false);
        }
        if !format!("{:x}", digest.finalize()).eq_ignore_ascii_case(&asset.md5) {
            return Err(format!("文件 MD5 与官方清单不符：{}", asset.path));
        }
        require_absent(path)?;
        fs::rename(partial, path).map_err(io_error)?;
        Ok(true)
    }

    fn verified_prefix(
        &self,
        file: &mut File,
        chunks: &[sophon::Chunk],
    ) -> Result<Option<(usize, u64, md5::Context)>, String> {
        let size = file.metadata().map_err(io_error)?.len();
        let mut digest = md5::Context::new();
        let mut length = 0;
        let mut finished = 0;
        let mut buffer = Vec::new();
        for chunk in chunks {
            if self.control.is_paused() {
                return Ok(None);
            }
            if chunk.offset + chunk.size > size {
                break;
            }
            buffer.resize(
                usize::try_from(chunk.size).map_err(|error| error.to_string())?,
                0,
            );
            file.read_exact(&mut buffer).map_err(io_error)?;
            if !format!("{:x}", md5::compute(&buffer)).eq_ignore_ascii_case(&chunk.md5) {
                break;
            }
            digest.consume(&buffer);
            length += chunk.size;
            finished += 1;
        }
        Ok(Some((finished, length, digest)))
    }
}

async fn wait_for_pause(control: &InstallControl) {
    while !control.is_paused() {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

fn validate_path(path: &str) -> Result<(), String> {
    for name in path.split(['/', '\\']) {
        if name.is_empty()
            || name.ends_with(['.', ' '])
            || name.eq_ignore_ascii_case(MARKER)
            || name
                .chars()
                .any(|c| c.is_control() || ":*?\"<>|".contains(c))
        {
            return Err(format!("官方清单包含不安全的 Windows 路径：{path}"));
        }
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end()
            .to_ascii_uppercase();
        if matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(
                    suffix,
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        {
            return Err(format!("官方清单包含 Windows 保留设备名：{path}"));
        }
    }
    Ok(())
}

fn create_subdirectories(root: &Path, path: &Path) -> Result<(), String> {
    let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        current.push(part);
        ensure_directory(&current)?;
    }
    Ok(())
}

fn is_reparse_point(metadata: &Metadata) -> bool {
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn ensure_directory(path: &Path) -> Result<(), String> {
    match optional(fs::symlink_metadata(path))? {
        Some(metadata) if metadata.is_dir() && !is_reparse_point(&metadata) => Ok(()),
        Some(_) => Err(format!("安装路径不是普通目录：{}", path.display())),
        None => fs::create_dir(path).map_err(io_error),
    }
}

fn regular_file(path: &Path) -> Result<Option<Metadata>, String> {
    optional(fs::symlink_metadata(path))?
        .map(|metadata| {
            if metadata.is_file() && !is_reparse_point(&metadata) {
                Ok(metadata)
            } else {
                Err(format!("安装路径不是普通文件：{}", path.display()))
            }
        })
        .transpose()
}

fn require_absent(path: &Path) -> Result<(), String> {
    if optional(fs::symlink_metadata(path))?.is_some() {
        return Err(format!("安装路径已存在：{}", path.display()));
    }
    Ok(())
}

fn optional<T>(result: io::Result<T>) -> Result<Option<T>, String> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Converts owned io::Error values in Result::map_err."
)]
fn io_error(error: io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_install::Progress;
    use prost::Message;
    use std::{thread, time::Duration};

    #[test]
    fn retries_default_save_after_publishing_without_redownloading() -> Result<(), String> {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("genshin-go-install-test-{seed}"));
        fs::create_dir(&root).map_err(io_error)?;
        let settings = root.join("settings.json");
        let journal = settings.with_file_name(JOURNAL);
        let mut job = Job::new(&root)?;
        let destination = job.destination();
        let stage = job.staging();
        fs::create_dir(&stage).map_err(io_error)?;

        let categories = [
            (
                "game",
                vec![
                    sophon::Asset {
                        path: "YuanShen.exe".into(),
                        md5: "d41d8cd98f00b204e9800998ecf8427e".into(),
                        ..Default::default()
                    },
                    sophon::Asset {
                        path: "YuanShen_Data".into(),
                        directory: true,
                        ..Default::default()
                    },
                ],
            ),
            (
                "zh-cn",
                vec![sophon::Asset {
                    path: "YuanShen_Data/Chinese".into(),
                    directory: true,
                    ..Default::default()
                }],
            ),
        ];
        let mut manifests = Vec::new();
        for (matching_field, assets) in categories {
            let bytes = (sophon::Manifest { assets }).encode_to_vec();
            let compressed = zstd::stream::encode_all(bytes.as_slice(), 1).map_err(io_error)?;
            fs::write(stage.join(format!("{matching_field}.pb.zst")), &compressed)
                .map_err(io_error)?;
            let source = serde_json::json!({
                "url_prefix": "", "url_suffix": "", "compression": 1, "encryption": 0
            });
            manifests.push(serde_json::json!({
                "matching_field": matching_field,
                "manifest": {
                    "id": "fixture", "checksum": format!("{:x}", md5::compute(&bytes)),
                    "compressed_size": compressed.len().to_string(),
                    "uncompressed_size": bytes.len().to_string()
                },
                "manifest_download": source, "chunk_download": source
            }));
        }
        job.build = Some(
            serde_json::from_value(serde_json::json!({
                "tag": "7.2.0", "build_id": "fixture", "manifests": manifests
            }))
            .map_err(|error| error.to_string())?,
        );
        job.save(&journal)?;

        let outcome = install(&settings, None, &InstallControl::default(), |_| {
            Err("配置不可写".into())
        });
        assert_eq!(outcome, Err("配置不可写".into()));
        assert_eq!(pending(&settings)?, Some(destination.clone()));
        assert!(destination.join("YuanShen_Data/Chinese").is_dir());
        assert_eq!(validate_installation(&destination)?, destination);

        let outcome = install(&settings, None, &InstallControl::default(), |directory| {
            crate::native_environment::save_default_at(&settings, directory)
        })?;
        assert_eq!(outcome, Some(destination.clone()));
        assert_eq!(pending(&settings)?, None);
        assert!(!stage.exists());
        assert!(!destination.join(MARKER).exists());
        fs::remove_file(destination.join("config.ini")).map_err(io_error)?;
        fs::remove_file(destination.join("YuanShen.exe")).map_err(io_error)?;
        let data = destination.join("YuanShen_Data");
        fs::remove_file(data.join("Persistent/audio_lang_14")).map_err(io_error)?;
        for directory in [
            data.join("Persistent"),
            data.join("Chinese"),
            data,
            destination,
        ] {
            fs::remove_dir(directory).map_err(io_error)?;
        }
        fs::remove_file(settings).map_err(io_error)?;
        fs::remove_file(journal.with_extension("lock")).map_err(io_error)?;
        fs::remove_dir(root).map_err(io_error)?;
        Ok(())
    }

    #[test]
    #[ignore = "Downloads official game resources into an explicitly supplied validation directory."]
    fn official_installation() -> Result<(), String> {
        let root = std::env::var_os("GENSHIN_GO_INSTALL_PROBE_ROOT")
            .map(PathBuf::from)
            .ok_or("需要设置 GENSHIN_GO_INSTALL_PROBE_ROOT")?;
        let mode =
            std::env::var("GENSHIN_GO_INSTALL_PROBE_STEP").map_err(|error| error.to_string())?;
        let target = root.join("games");
        let settings = root.join("profile/settings.json");
        fs::create_dir_all(&target).map_err(io_error)?;
        let saved = pending(&settings)?;
        let parent = match mode.as_str() {
            "pause" if saved.is_none() => Some(target),
            "resume" | "finish" if saved.is_some() => None,
            _ => return Err("需要依次执行 pause、resume 和 finish".into()),
        };
        let control = InstallControl::default();
        thread::scope(|scope| {
            let task = scope.spawn(|| {
                install(&settings, parent.as_deref(), &control, |directory| {
                    crate::native_environment::save_default_at(&settings, directory)
                })
            });
            let stop_after = match mode.as_str() {
                "pause" => 32 * 1024 * 1024,
                "resume" => 96 * 1024 * 1024,
                _ => u64::MAX,
            };
            let mut last_report = 0;
            while !task.is_finished() {
                if let Some(Progress { written, total }) = control.progress() {
                    if written >= stop_after {
                        control.pause();
                    }
                    if written.saturating_sub(last_report) >= 16 * 1024 * 1024 {
                        eprintln!("已写入 {written} / {total} 字节");
                        last_report = written;
                    }
                }
                thread::sleep(Duration::from_millis(200));
            }
            let result = task.join().map_err(|_| "安装线程失败")??;
            if mode == "finish" {
                let path = result.ok_or("安装未完成")?;
                if pending(&settings)?.is_some() {
                    return Err("安装完成后仍有待处理的任务".into());
                }
                eprintln!("已安装到 {}", path.display());
            } else {
                let Progress { written, total } = control.progress().ok_or("未收到下载进度")?;
                if result.is_some() || written < stop_after || pending(&settings)?.is_none() {
                    return Err("未保存暂停的安装记录".into());
                }
                eprintln!("暂停后已保存 {written} / {total} 字节");
            }
            Ok(())
        })
    }
}
