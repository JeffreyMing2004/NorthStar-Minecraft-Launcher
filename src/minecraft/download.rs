use super::{ProxyMode, Result, Source};
use reqwest::blocking::{Client, ClientBuilder};
use serde::de::DeserializeOwned;
use sha1::{Digest, Sha1};
use std::{
    error::Error,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Default, Debug)]
pub struct Progress {
    pub stage: String,
    pub completed: usize,
    pub total: usize,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

pub type Reporter = Arc<dyn Fn(Progress) + Send + Sync>;

#[derive(Clone)]
pub struct Downloader {
    pub source: Source,
    pub cancel: Arc<AtomicBool>,
    client: Client,
    reporter: Reporter,
    progress: Arc<Mutex<(Progress, Instant)>>,
}

#[derive(Clone, Debug)]
pub struct Job {
    pub url: String,
    pub path: PathBuf,
    pub sha1: String,
    pub size: Option<u64>,
}

impl Downloader {
    pub fn new(source: Source, cancel: Arc<AtomicBool>, reporter: Reporter) -> Result<Self> {
        let base = source.base()?;
        let builder = Client::builder()
            .user_agent(concat!("NSCL/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(180))
            .redirect(reqwest::redirect::Policy::limited(8));
        let local = base
            .as_ref()
            .and_then(|url| url.host_str())
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1"));
        let proxy_mode = if local {
            ProxyMode::Direct
        } else {
            source.proxy_mode
        };
        let client = configure_proxy(builder, proxy_mode)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            source,
            cancel,
            client,
            reporter,
            progress: Arc::new(Mutex::new((Progress::default(), Instant::now()))),
        })
    }

    pub fn check_cancelled(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err("操作已取消。".into())
        } else {
            Ok(())
        }
    }

    pub fn phase(&self, stage: &str, total: usize) {
        let event = {
            let mut state = self.progress.lock().unwrap();
            state.0.stage = stage.into();
            state.0.completed = 0;
            state.0.total = total;
            state.0.total_bytes = None;
            state.0.clone()
        };
        (self.reporter)(event);
    }

    pub(crate) fn byte_progress(&self, stage: &str, downloaded: u64, total: u64) {
        (self.reporter)(Progress {
            stage: stage.into(),
            bytes_downloaded: downloaded,
            total_bytes: Some(total),
            ..Default::default()
        });
    }

    pub(crate) fn complete(&self, stage: &str) {
        self.update(stage, 0, true);
    }

    fn update(&self, stage: &str, bytes: u64, complete: bool) {
        let event = {
            let mut state = self.progress.lock().unwrap();
            state.0.bytes_downloaded += bytes;
            if complete {
                state.0.completed += 1;
            }
            if complete || state.1.elapsed() > Duration::from_millis(150) {
                state.0.stage = stage.to_owned();
                state.1 = Instant::now();
                Some(state.0.clone())
            } else {
                None
            }
        };
        if let Some(event) = event {
            (self.reporter)(event);
        }
    }

    pub fn json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        self.fetch(url, |bytes| {
            serde_json::from_slice(bytes).map_err(|e| format!("元数据不是有效 JSON：{e}"))
        })
    }

    pub(super) fn checksum(&self, url: &str) -> Result<String> {
        self.fetch(url, |bytes| {
            let text = std::str::from_utf8(bytes).map_err(|_| "校验文件编码无效。")?;
            let hash = text.split_whitespace().next().unwrap_or("");
            validate_hash(hash)?;
            Ok(hash.to_ascii_lowercase())
        })
    }

    pub(super) fn fetch<T>(&self, url: &str, parse: impl Fn(&[u8]) -> Result<T>) -> Result<T> {
        let mut errors = Vec::new();
        for candidate in self.source.candidates(url)? {
            self.check_cancelled()?;
            let result = (|| {
                let response = self
                    .client
                    .get(candidate.as_str())
                    .send()
                    .and_then(|r| r.error_for_status())
                    .map_err(network_error)?;
                let mut bytes = Vec::new();
                response
                    .take(16 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 16 * 1024 * 1024 {
                    return Err("版本列表过大。".into());
                }
                parse(&bytes)
            })();
            match result {
                Ok(data) => return Ok(data),
                Err(error) => errors.push(format!("{candidate}: {error}")),
            }
        }
        Err(errors.join("\n"))
    }

    pub fn download(&self, job: &Job) -> Result<()> {
        if job.url.is_empty() {
            validate_hash(&job.sha1)?;
            self.check_cancelled()?;
            return if verified(&job.path, &job.sha1, job.size)? {
                Ok(())
            } else {
                Err(format!(
                    "安装生成的文件缺失或损坏，请重新安装加载器：{}",
                    job.path.display()
                ))
            };
        }
        self.download_from(job, &self.source.candidates(&job.url)?)
    }

    pub(super) fn download_from(&self, job: &Job, candidates: &[String]) -> Result<()> {
        validate_hash(&job.sha1)?;
        self.check_cancelled()?;
        if verified(&job.path, &job.sha1, job.size)? {
            return Ok(());
        }
        let parent = job.path.parent().ok_or("无效下载路径")?;
        fs::create_dir_all(parent).map_err(|e| format!("创建下载目录失败：{e}"))?;
        let temporary = job.path.with_file_name(format!(
            "{}.part",
            job.path.file_name().unwrap().to_string_lossy()
        ));
        let mut errors = Vec::new();
        for candidate in candidates {
            for _ in 0..2 {
                self.check_cancelled()?;
                let result = (|| {
                    let mut response = self
                        .client
                        .get(candidate.as_str())
                        .send()
                        .and_then(|r| r.error_for_status())
                        .map_err(network_error)?;
                    let mut file = File::create(&temporary).map_err(|e| e.to_string())?;
                    let mut hash = Sha1::new();
                    let mut size = 0u64;
                    let mut buffer = [0u8; 65536];
                    loop {
                        self.check_cancelled()?;
                        let read = response.read(&mut buffer).map_err(|e| e.to_string())?;
                        if read == 0 {
                            break;
                        }
                        size += read as u64;
                        if size > job.size.unwrap_or(2 * 1024 * 1024 * 1024) {
                            return Err("文件长度超出元数据记录，已拒绝保存。".into());
                        }
                        file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
                        hash.update(&buffer[..read]);
                        self.update(
                            &format!("下载 {}", job.path.file_name().unwrap().to_string_lossy()),
                            read as u64,
                            false,
                        );
                    }
                    if job.size.is_some_and(|expected| expected != size)
                        || format!("{:x}", hash.finalize()) != job.sha1.to_ascii_lowercase()
                    {
                        return Err("SHA-1 或文件长度校验失败。".into());
                    }
                    file.sync_all().map_err(|e| e.to_string())?;
                    drop(file);
                    replace_file(&temporary, &job.path)
                })();
                match result {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        let _ = fs::remove_file(&temporary);
                        self.check_cancelled()?;
                        errors.push(format!("{candidate}: {error}"));
                    }
                }
            }
        }
        Err(format!(
            "下载失败：{}\n{}",
            job.path.display(),
            errors.join("\n")
        ))
    }

    pub fn many(&self, jobs: &[Job]) -> Result<()> {
        self.phase("校验与下载游戏文件", jobs.len());
        let next = AtomicUsize::new(0);
        let error = Mutex::new(None);
        std::thread::scope(|scope| {
            for _ in 0..4.min(jobs.len()) {
                scope.spawn(|| {
                    loop {
                        if error.lock().unwrap().is_some() {
                            break;
                        }
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(index) else {
                            break;
                        };
                        match self.download(job) {
                            Ok(()) => self.update("校验与下载游戏文件", 0, true),
                            Err(message) => {
                                let mut first = error.lock().unwrap();
                                if first.is_none() {
                                    *first = Some(message);
                                }
                                break;
                            }
                        }
                    }
                });
            }
        });
        self.check_cancelled()?;
        if let Some(error) = error.into_inner().unwrap() {
            Err(error)
        } else {
            Ok(())
        }
    }
}

fn configure_proxy(builder: ClientBuilder, mode: ProxyMode) -> ClientBuilder {
    match mode {
        ProxyMode::System => builder,
        ProxyMode::Direct => builder.no_proxy(),
    }
}

fn network_error(error: reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(error) = cause {
        message.push_str(&format!("：{error}"));
        cause = error.source();
    }
    message
}

pub fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 40 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Err("元数据缺少有效的 SHA-1 校验值。".into())
    } else {
        Ok(())
    }
}

pub fn verified(path: &Path, expected: &str, size: Option<u64>) -> Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    if size.is_some_and(|size| file.metadata().map_or(true, |meta| meta.len() != size)) {
        return Ok(false);
    }
    let mut hash = Sha1::new();
    let mut buffer = [0; 65536];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()) == expected.to_ascii_lowercase())
}

pub fn safe_join(root: &Path, value: &str) -> Result<PathBuf> {
    if value.is_empty()
        || value.contains(['\\', ':', '\0', '*', '?', '"', '<', '>', '|'])
        || value.split('/').any(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            part.is_empty()
                || part == "."
                || part == ".."
                || part.ends_with(['.', ' '])
                || part.chars().any(char::is_control)
                || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || (stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        })
        || Path::new(value)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("拒绝不安全的文件路径：{value}"));
    }
    Ok(root.join(value))
}

pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-+".contains(&c))
    {
        Err("无效的游戏版本标识。".into())
    } else {
        safe_join(Path::new(""), id).map(|_| ())
    }
}

pub fn validate_local_id(id: &str) -> Result<()> {
    if id.is_empty() || id.contains('/') || id.encode_utf16().count() > 255 {
        return Err("无效的本地版本目录名称。".into());
    }
    safe_join(Path::new(""), id).map(|_| ())
}

pub fn replace_file(temporary: &Path, path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    fs::rename(temporary, path).map_err(|e| e.to_string())
}

pub fn write_json(path: &Path, data: &impl serde::Serialize) -> Result<()> {
    let temporary = path.with_extension("json.part");
    fs::create_dir_all(path.parent().ok_or("无效文件路径")?).map_err(|e| e.to_string())?;
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    replace_file(&temporary, path)
}

#[cfg(test)]
mod proxy_tests {
    use super::*;

    #[test]
    fn direct_mode_bypasses_a_proxy_while_system_mode_preserves_it() {
        let target = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let proxy = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let target_url = format!("http://{}", target.server_addr());
        let proxy_url = format!("http://{}", proxy.server_addr());
        std::thread::scope(|scope| {
            for server in [&target, &proxy] {
                scope.spawn(move || {
                    let request = server
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap()
                        .unwrap();
                    request
                        .respond(tiny_http::Response::from_string("ok"))
                        .unwrap();
                });
            }
            for mode in [ProxyMode::System, ProxyMode::Direct] {
                let client = configure_proxy(
                    Client::builder().proxy(reqwest::Proxy::all(&proxy_url).unwrap()),
                    mode,
                )
                .build()
                .unwrap();
                assert_eq!(
                    client.get(&target_url).send().unwrap().text().unwrap(),
                    "ok"
                );
            }
        });
    }
}
