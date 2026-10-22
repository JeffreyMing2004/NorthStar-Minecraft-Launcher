//! Public GitHub Releases updates. No GitHub credentials are stored or required.
use crate::minecraft::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const REPOSITORY: &str = "JeffreyMing2004/NorthStar-Minecraft-Launcher";
pub const RELEASES_URL: &str =
    "https://github.com/JeffreyMing2004/NorthStar-Minecraft-Launcher/releases";
const MAX_ASSET: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
    pub digest: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub html_url: String,
    #[serde(default)]
    pub body: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}
impl Release {
    pub fn newer_than(&self, current: &str) -> Result<bool> {
        let version = |v: &str| {
            semver::Version::parse(v.trim().trim_start_matches('v'))
                .map_err(|_| "发布版本号无效，请使用 v1.2.3 格式。".to_string())
        };
        let latest = version(&self.tag_name)?;
        Ok(!self.draft && !self.prerelease && latest.pre.is_empty() && latest > version(current)?)
    }
    pub fn asset(&self) -> Option<&Asset> {
        if !cfg!(windows) {
            return None;
        }
        let arch = std::env::consts::ARCH;
        self.assets
            .iter()
            .filter(|a| {
                let n = a.name.to_ascii_lowercase();
                n.starts_with("nscl")
                    && (n.ends_with(".zip") || n.ends_with(".exe"))
                    && n.contains("windows")
                    && (n.contains(arch) || (arch == "x86_64" && n.contains("x64")))
                    && !n.contains("setup")
                    && !n.contains("installer")
            })
            .min_by_key(|a| (!a.name.ends_with(".zip"), &a.name))
    }
}

fn client() -> Result<Client> {
    Client::builder()
        .user_agent(concat!("NSCL/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let u = attempt.url();
            if attempt.previous().len() < 5
                && u.scheme() == "https"
                && u.host_str().is_some_and(|h| {
                    h == "github.com"
                        || h == "api.github.com"
                        || h.ends_with(".githubusercontent.com")
                })
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|e| format!("更新连接初始化失败：{e}"))
}

pub fn check() -> Result<Option<Release>> {
    let response = client()?
        .get(format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|_| "无法连接 GitHub Releases，请稍后重试。")?;
    if response.status().as_u16() == 404 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!(
            "GitHub 更新检查失败（HTTP {}）。",
            response.status().as_u16()
        ));
    }
    let release: Release = serde_json::from_reader(response.take(2 * 1024 * 1024))
        .map_err(|_| "GitHub 发布信息无效。")?;
    validate_release(&release)?;
    Ok(Some(release))
}

fn validate_release(release: &Release) -> Result<()> {
    if !release
        .html_url
        .starts_with(&format!("{RELEASES_URL}/tag/"))
        || release.draft
        || release.prerelease
    {
        return Err("发布来源或发布通道无效。".into());
    }
    release.newer_than(env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
fn validate_asset(asset: &Asset) -> Result<()> {
    let url = reqwest::Url::parse(&asset.browser_download_url).map_err(|_| "更新下载地址无效。")?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url
            .path()
            .starts_with(&format!("/{REPOSITORY}/releases/download/"))
        || !url.username().is_empty()
        || url.password().is_some()
        || asset.name.is_empty()
        || asset.name.len() > 160
        || !asset
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || asset.size == 0
        || asset.size > MAX_ASSET
    {
        return Err("更新资源不属于官方仓库或大小/名称无效。".into());
    }
    Ok(())
}
fn parse_hash(text: &str) -> Result<String> {
    let hash = text
        .strip_prefix("sha256:")
        .unwrap_or(text)
        .to_ascii_lowercase();
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("发布资源缺少有效的 SHA-256 校验值。".into());
    }
    Ok(hash)
}

pub fn cache_directory() -> PathBuf {
    crate::store::settings_path()
        .with_file_name("cache")
        .join("updates")
}
#[derive(Debug)]
pub struct ReadyUpdate {
    pub executable: PathBuf,
    pub sha256: String,
}
pub fn download(
    release: &Release,
    cancel: &AtomicBool,
    progress: impl Fn(u64, u64),
) -> Result<ReadyUpdate> {
    if cancel.load(Ordering::Relaxed) {
        return Err("更新下载已取消。".into());
    }
    validate_release(release)?;
    if !release.newer_than(env!("CARGO_PKG_VERSION"))? {
        return Err("已是当前版本，无需下载或降级。".into());
    }
    let asset = release
        .asset()
        .ok_or("此发布没有适用于当前系统的 NSCL 便携更新包，请打开发布页查看。")?;
    validate_asset(asset)?;
    let client = client()?;
    let expected = if let Some(digest) = &asset.digest {
        parse_hash(digest)?
    } else {
        let sums = release
            .assets
            .iter()
            .find(|a| a.name == "SHA256SUMS")
            .ok_or("此发布没有 SHA-256 校验值，请在发布页手动获取。")?;
        validate_asset(sums)?;
        if sums.size > 128 * 1024 {
            return Err("校验清单过大。".into());
        }
        let response = client
            .get(&sums.browser_download_url)
            .send()
            .map_err(|_| "读取更新校验清单失败。")?
            .error_for_status()
            .map_err(|_| "读取更新校验清单失败。")?;
        let mut text = String::new();
        response
            .take(128 * 1024)
            .read_to_string(&mut text)
            .map_err(|_| "校验清单编码无效。")?;
        let hash = text
            .lines()
            .find_map(|line| {
                let mut p = line.split_whitespace();
                let h = p.next()?;
                (p.next()?.trim_start_matches('*') == asset.name).then_some(h)
            })
            .ok_or("校验清单中没有对应的更新包。")?;
        parse_hash(hash)?
    };
    let directory = cache_directory().join(uuid::Uuid::new_v4().simple().to_string());
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let archive = directory.join(&asset.name);
    let result = (|| {
        let response = client
            .get(&asset.browser_download_url)
            .send()
            .map_err(|_| "下载更新失败，请重试。")?
            .error_for_status()
            .map_err(|_| "下载更新失败，请重试。")?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&archive)
            .map_err(|e| e.to_string())?;
        receive_asset(
            response,
            &mut output,
            asset.size,
            &expected,
            cancel,
            &progress,
        )?;
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        let executable = directory.join("payload.exe");
        extract_payload(&archive, &executable)?;
        if cancel.load(Ordering::Relaxed) {
            return Err("更新下载已取消。".into());
        }
        let sha256 = file_hash(&executable)?;
        Ok(ReadyUpdate { executable, sha256 })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&archive);
    }
    result
}

fn receive_asset(
    mut source: impl Read,
    output: &mut impl Write,
    size: u64,
    expected: &str,
    cancel: &AtomicBool,
    progress: &impl Fn(u64, u64),
) -> Result<()> {
    let mut hash = Sha256::new();
    let mut done = 0;
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("更新下载已取消。".into());
        }
        let n = source
            .read(&mut buffer)
            .map_err(|_| "更新下载中断，请重试。")?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > size {
            return Err("更新资源大小与发布信息不一致。".into());
        }
        hash.update(&buffer[..n]);
        output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        progress(done, size);
    }
    if done != size || format!("{:x}", hash.finalize()) != expected {
        return Err("更新包 SHA-256 或文件长度校验失败，未执行更新。".into());
    }
    Ok(())
}

fn extract_payload(archive: &Path, output: &Path) -> Result<()> {
    if archive
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        fs::copy(archive, output).map_err(|e| e.to_string())?;
    } else {
        let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(|e| e.to_string())?)
            .map_err(|_| "更新 ZIP 损坏。")?;
        let mut candidates = Vec::new();
        for i in 0..zip.len() {
            let entry = zip.by_index(i).map_err(|e| e.to_string())?;
            if !entry.is_dir()
                && entry
                    .name()
                    .rsplit('/')
                    .next()
                    .is_some_and(|n| n.eq_ignore_ascii_case("nscl.exe"))
            {
                candidates.push(i);
            }
        }
        if candidates.len() != 1 {
            return Err("更新包必须包含唯一的 NSCL.exe。".into());
        }
        let entry = zip.by_index(candidates[0]).map_err(|e| e.to_string())?;
        if entry.size() > MAX_ASSET {
            return Err("解压后的更新程序过大。".into());
        }
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output)
            .map_err(|e| e.to_string())?;
        let size =
            std::io::copy(&mut entry.take(MAX_ASSET + 1), &mut file).map_err(|e| e.to_string())?;
        if size > MAX_ASSET {
            return Err("解压后的更新程序过大。".into());
        }
        file.sync_all().map_err(|e| e.to_string())?;
    }
    validate_executable(output)
}

fn validate_executable(path: &Path) -> Result<()> {
    let mut header = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(1024 * 1024)
        .read_to_end(&mut header)
        .map_err(|e| e.to_string())?;
    let invalid = || "更新文件不是适用于当前架构的 Windows 程序。".to_string();
    if header.get(..2) != Some(b"MZ") {
        return Err(invalid());
    }
    let offset =
        u32::from_le_bytes(header.get(60..64).ok_or_else(invalid)?.try_into().unwrap()) as usize;
    if header.get(offset..offset + 4) != Some(b"PE\0\0") {
        return Err(invalid());
    }
    let machine = u16::from_le_bytes(
        header
            .get(offset + 4..offset + 6)
            .ok_or_else(invalid)?
            .try_into()
            .unwrap(),
    );
    let expected = match std::env::consts::ARCH {
        "x86_64" => 0x8664,
        "aarch64" => 0xaa64,
        _ => return Err(invalid()),
    };
    if machine != expected {
        return Err(invalid());
    }
    Ok(())
}

fn file_hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[derive(Serialize, Deserialize)]
struct ApplyPlan {
    parent_pid: u32,
    target: PathBuf,
    payload: PathBuf,
    old_hash: String,
    new_hash: String,
}

pub fn restart_with_update(update: &ReadyUpdate) -> Result<()> {
    if !cfg!(windows) {
        return Err("当前系统请从发布页手动更新。".into());
    }
    let payload = &update.executable;
    if file_hash(payload)? != update.sha256 {
        return Err("已下载的更新文件发生变化，请重新下载。".into());
    }
    validate_executable(payload)?;
    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    let directory = payload.parent().ok_or("更新目录无效。")?;
    let helper = directory.join("update-helper.exe");
    fs::copy(&target, &helper).map_err(|e| format!("创建更新辅助程序失败：{e}"))?;
    let plan = ApplyPlan {
        parent_pid: std::process::id(),
        old_hash: file_hash(&target)?,
        new_hash: update.sha256.clone(),
        target,
        payload: payload.into(),
    };
    let path = directory.join("apply.json");
    crate::store::write_json(&path, &plan)?;
    crate::minecraft::hidden_command(&helper)
        .arg("--nscl-update-helper")
        .arg(path)
        .spawn()
        .map_err(|e| format!("启动更新程序失败：{e}"))?;
    Ok(())
}

pub fn run_helper() -> Option<Result<()>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--nscl-update-helper")) {
        return None;
    }
    Some((|| {
        let path = PathBuf::from(args.next().ok_or("更新计划缺失。")?);
        let plan: ApplyPlan = serde_json::from_reader(
            fs::File::open(&path)
                .map_err(|e| e.to_string())?
                .take(65536),
        )
        .map_err(|_| "更新计划无效。")?;
        let result = apply(&plan);
        if let Err(error) = &result {
            let _ = fs::write(cache_directory().join("last-error.txt"), error);
            let _ = crate::minecraft::hidden_command(&plan.target).spawn();
        }
        result
    })())
}

fn apply(plan: &ApplyPlan) -> Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_OBJECT_0},
            System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
        };
        // SAFETY: only wait for the parent process; no process or token is modified.
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, plan.parent_pid) };
        if !process.is_null() {
            let result = unsafe { WaitForSingleObject(process, 60000) };
            unsafe {
                CloseHandle(process);
            }
            if result != WAIT_OBJECT_0 {
                return Err("等待启动器退出超时，请手动重试更新。".into());
            }
        } else if std::io::Error::last_os_error().raw_os_error() != Some(87) {
            return Err("无法等待原启动器退出。".into());
        }
    }
    let backup = replace_payload(plan)?;
    if let Err(error) = crate::minecraft::hidden_command(&plan.target).spawn() {
        let _ = fs::remove_file(&plan.target);
        let _ = fs::rename(&backup, &plan.target);
        return Err(format!("新版无法启动，已恢复旧版：{error}"));
    }
    let _ = fs::remove_file(cache_directory().join("last-error.txt"));
    Ok(())
}

fn replace_payload(plan: &ApplyPlan) -> Result<PathBuf> {
    if !plan.target.is_absolute()
        || plan
            .target
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("exe"))
        || file_hash(&plan.target)? != plan.old_hash
        || file_hash(&plan.payload)? != plan.new_hash
    {
        return Err("更新文件或原程序已经改变，已停止替换。".into());
    }
    validate_executable(&plan.payload)?;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let staged = plan.target.with_file_name(format!(".nscl-{suffix}.exe"));
    let backup = plan
        .target
        .with_file_name(format!(".nscl-backup-{suffix}.exe"));
    fs::copy(&plan.payload, &staged).map_err(|e| format!("写入启动器目录失败：{e}"))?;
    if file_hash(&staged)? != plan.new_hash {
        let _ = fs::remove_file(&staged);
        return Err("复制后的更新程序校验失败，已保留原程序。".into());
    }
    fs::rename(&plan.target, &backup).map_err(|e| format!("备份原程序失败：{e}"))?;
    if let Err(error) = fs::rename(&staged, &plan.target) {
        let _ = fs::rename(&backup, &plan.target);
        return Err(format!("替换失败，已恢复原程序：{error}"));
    }
    Ok(backup)
}

#[cfg(test)]
mod tests;
