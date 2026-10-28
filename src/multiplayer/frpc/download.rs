//! Download the official FRP release, verify SHA-256, and extract only frpc.
use crate::multiplayer::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const RELEASE: &str = "https://api.github.com/repos/fatedier/frp/releases/latest";
const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_EXECUTABLE: u64 = 128 * 1024 * 1024;
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    version: String,
    executable: String,
    sha256: String,
}

pub fn directory() -> PathBuf {
    crate::store::launcher_directory()
        .join("tools")
        .join("frpc")
}
pub fn installed(directory: &Path) -> Option<PathBuf> {
    let mut bytes = Vec::new();
    fs::File::open(directory.join("current.json"))
        .ok()?
        .take(4097)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    let receipt: Receipt = serde_json::from_slice(&bytes).ok()?;
    let path = crate::minecraft::safe_join(directory, &receipt.executable).ok()?;
    crate::instances::reject_link(&path).ok()?;
    if hash(&path).ok()? == receipt.sha256 {
        Some(path)
    } else {
        None
    }
}

pub fn install(
    directory: &Path,
    cancel: &Arc<AtomicBool>,
    report: &(dyn Fn(String) + Send + Sync),
) -> Result<PathBuf> {
    install_from(directory, cancel, report, RELEASE)
}
fn install_from(
    directory: &Path,
    cancel: &Arc<AtomicBool>,
    report: &(dyn Fn(String) + Send + Sync),
    release_url: &str,
) -> Result<PathBuf> {
    check(cancel)?;
    if let Some(path) = installed(directory) {
        report("frpc 已准备好。".into());
        return Ok(path);
    }
    report("正在获取 FRP 官方下载信息……".into());
    let client = client()?;
    let release: Release =
        serde_json::from_slice(&fetch(&client, release_url, 1024 * 1024, cancel)?)
            .map_err(|_| "FRP 官方发布信息无效。")?;
    if release.draft
        || release.prerelease
        || !release.tag_name.starts_with('v')
        || release.tag_name.len() > 64
        || !release
            .tag_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
    {
        return Err("FRP 官方版本标识无效。".into());
    }
    let version = release.tag_name.trim_start_matches('v');
    let target = target(std::env::consts::OS, std::env::consts::ARCH)?;
    let extension = if std::env::consts::OS == "windows" {
        "zip"
    } else {
        "tar.gz"
    };
    let name = format!("frp_{version}_{target}.{extension}");
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or("官方发布中没有适用于本机的 frpc，请选择已有程序。")?;
    if asset.size == 0 || asset.size > MAX_ARCHIVE {
        return Err("frpc 官方下载文件大小无效。".into());
    }
    validate_url(&asset.browser_download_url)?;
    let expected = match &asset.digest {
        Some(digest) => valid_hash(
            digest
                .strip_prefix("sha256:")
                .ok_or("frpc 下载缺少 SHA-256 校验信息。")?,
        )?,
        None => {
            let sums = release
                .assets
                .iter()
                .find(|a| a.name == "frp_sha256_checksums.txt")
                .ok_or("官方发布缺少 SHA-256 校验文件。")?;
            validate_url(&sums.browser_download_url)?;
            let bytes = fetch(&client, &sums.browser_download_url, 128 * 1024, cancel)?;
            checksum(
                &String::from_utf8(bytes).map_err(|_| "frpc 校验文件无效。")?,
                &name,
            )?
        }
    };
    crate::instances::reject_link(directory)?;
    fs::create_dir_all(directory).map_err(|_| "无法创建 frpc 下载目录。")?;
    let stage = directory.join(format!(".frpc-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir(&stage).map_err(|_| "无法创建 frpc 暂存目录。")?;
    let archive = stage.join("archive");
    let extracted = stage.join(if cfg!(windows) { "frpc.exe" } else { "frpc" });
    let _guard = Stage {
        directory: stage.clone(),
        archive: archive.clone(),
        executable: extracted.clone(),
    };
    report(format!("正在下载 frpc {version}……"));
    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .map_err(|_| "连接 FRP 官方下载地址失败，请检查网络后重试。")?
        .error_for_status()
        .map_err(|_| "FRP 官方下载暂不可用，请稍后重试。")?;
    if response.content_length().is_some_and(|n| n != asset.size) {
        return Err("frpc 下载大小与官方发布不一致。".into());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&archive)
        .map_err(|_| "无法写入 frpc 下载文件。")?;
    let mut digest = Sha256::new();
    let mut received = 0u64;
    let mut buffer = [0; 65536];
    let mut last = std::time::Instant::now();
    loop {
        check(cancel)?;
        let n = response
            .read(&mut buffer)
            .map_err(|_| "frpc 下载中断，请重试。")?;
        if n == 0 {
            break;
        }
        received += n as u64;
        if received > asset.size {
            return Err("frpc 下载文件超过官方标注大小。".into());
        }
        digest.update(&buffer[..n]);
        file.write_all(&buffer[..n])
            .map_err(|_| "保存 frpc 下载文件失败。")?;
        if last.elapsed() > Duration::from_millis(200) {
            report(format!("正在下载 frpc：{}%", received * 100 / asset.size));
            last = std::time::Instant::now();
        }
    }
    file.sync_all().map_err(|_| "保存 frpc 下载文件失败。")?;
    drop(file);
    if received != asset.size || format!("{:x}", digest.finalize()) != expected {
        return Err("frpc 下载未通过 SHA-256 校验，请重试。".into());
    }
    check(cancel)?;
    report("下载完成，正在校验并解压 frpc……".into());
    extract(&archive, &extracted, extension, cancel)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&extracted, fs::Permissions::from_mode(0o755))
            .map_err(|_| "无法设置 frpc 执行权限。")?;
    }
    let hash = hash(&extracted)?;
    let relative = format!(
        "{}/{}/{}",
        release.tag_name,
        target,
        if cfg!(windows) { "frpc.exe" } else { "frpc" }
    );
    let destination = crate::minecraft::safe_join(directory, &relative)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|_| "无法创建 frpc 程序目录。")?;
    }
    crate::instances::reject_link(&destination)?;
    check(cancel)?;
    fs::rename(&extracted, &destination).map_err(|_| "无法安装 frpc，文件可能正在使用。")?;
    crate::store::write_json(
        &directory.join("current.json"),
        &Receipt {
            version: release.tag_name,
            executable: relative,
            sha256: hash,
        },
    )?;
    report("frpc 下载和校验完成，可创建联机房间。".into());
    Ok(destination)
}

fn target(os: &str, arch: &str) -> Result<String> {
    let os = match os {
        "windows" => "windows",
        "macos" => "darwin",
        "linux" => "linux",
        _ => return Err("当前系统请手动选择 frpc 程序。".into()),
    };
    let arch = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        _ => return Err("当前处理器请手动选择 frpc 程序。".into()),
    };
    Ok(format!("{os}_{arch}"))
}
fn client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .user_agent(concat!(
            "NorthStar-Minecraft-Launcher/",
            env!("CARGO_PKG_VERSION")
        ))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.stop();
            }
            if attempt.url().scheme() == "https"
                && matches!(
                    attempt.url().host_str(),
                    Some(
                        "github.com"
                            | "api.github.com"
                            | "release-assets.githubusercontent.com"
                            | "objects.githubusercontent.com"
                    )
                )
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|_| "无法创建 frpc 下载连接。".into())
}
fn fetch(client: &Client, url: &str, limit: u64, cancel: &Arc<AtomicBool>) -> Result<Vec<u8>> {
    validate_url(url)?;
    check(cancel)?;
    let response = client
        .get(url)
        .send()
        .map_err(|_| "无法连接 FRP 官方下载服务。")?
        .error_for_status()
        .map_err(|_| "FRP 官方下载服务暂不可用。")?;
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "读取 FRP 发布信息失败。")?;
    check(cancel)?;
    if bytes.len() as u64 > limit {
        return Err("FRP 发布信息过大。".into());
    }
    Ok(bytes)
}
fn validate_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).map_err(|_| "FRP 下载地址无效。")?;
    if cfg!(test) && url.scheme() == "http" && url.host_str() == Some("127.0.0.1") {
        return Ok(());
    }
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
        || !((url.host_str() == Some("api.github.com")
            && url.path() == "/repos/fatedier/frp/releases/latest")
            || url.host_str() == Some("github.com")
                && url.path().starts_with("/fatedier/frp/releases/download/"))
    {
        return Err("frpc 仅从 FRP 官方 GitHub 发布下载。".into());
    }
    Ok(())
}
fn valid_hash(hash: &str) -> Result<String> {
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("frpc SHA-256 校验值无效。".into());
    }
    Ok(hash.to_ascii_lowercase())
}
fn checksum(text: &str, name: &str) -> Result<String> {
    let hashes: Vec<_> = text
        .lines()
        .filter_map(|l| {
            let mut s = l.split_whitespace();
            let hash = s.next()?;
            let file = s.next()?.trim_start_matches('*');
            (file == name).then_some(hash)
        })
        .collect();
    if hashes.len() != 1 {
        return Err("frpc 校验文件没有唯一匹配的下载文件。".into());
    }
    valid_hash(hashes[0])
}
fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|_| "无法读取 frpc 程序。")?;
    if file.metadata().map_err(|_| "无法读取 frpc 大小。")?.len() > MAX_EXECUTABLE {
        return Err("frpc 程序过大。".into());
    }
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let n = file.read(&mut bytes).map_err(|_| "无法校验 frpc 程序。")?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn extract(
    archive: &Path,
    destination: &Path,
    extension: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<()> {
    let name = if cfg!(windows) { "frpc.exe" } else { "frpc" };
    let mut found = false;
    let mut expanded = 0u64;
    let file = fs::File::open(archive).map_err(|_| "无法读取 frpc 压缩包。")?;
    if extension == "zip" {
        let mut zip = zip::ZipArchive::new(file).map_err(|_| "frpc ZIP 无效。")?;
        if zip.len() > 1024 {
            return Err("frpc 压缩包条目过多。".into());
        }
        for i in 0..zip.len() {
            check(cancel)?;
            let mut entry = zip.by_index(i).map_err(|_| "frpc ZIP 条目无效。")?;
            expanded = expanded.saturating_add(entry.size());
            if expanded > 512 * 1024 * 1024 {
                return Err("frpc 压缩包解压后过大。".into());
            }
            let path =
                crate::minecraft::safe_join(Path::new(""), entry.name().trim_end_matches('/'))?;
            if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                return Err("frpc ZIP 不能包含链接。".into());
            }
            if !entry.is_dir() && path.file_name().is_some_and(|n| n == name) {
                if found {
                    return Err("frpc 压缩包包含重复程序。".into());
                }
                if entry.size() > MAX_EXECUTABLE {
                    return Err("frpc 程序解压后过大。".into());
                }
                write_payload(&mut entry, MAX_EXECUTABLE, destination, cancel)?;
                found = true;
            }
        }
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for (index, entry) in tar.entries().map_err(|_| "frpc TAR 无效。")?.enumerate() {
            check(cancel)?;
            if index >= 1024 {
                return Err("frpc 压缩包条目过多。".into());
            }
            let mut entry = entry.map_err(|_| "frpc TAR 条目无效。")?;
            expanded = expanded.saturating_add(entry.size());
            if expanded > 512 * 1024 * 1024 {
                return Err("frpc 压缩包解压后过大。".into());
            }
            let relative = entry
                .path()
                .map_err(|_| "frpc TAR 路径无效。")?
                .to_string_lossy()
                .into_owned();
            let path = crate::minecraft::safe_join(Path::new(""), relative.trim_end_matches('/'))?;
            let kind = entry.header().entry_type();
            if !kind.is_file() && !kind.is_dir() {
                return Err("frpc TAR 不能包含链接或特殊文件。".into());
            }
            if kind.is_file() && path.file_name().is_some_and(|n| n == name) {
                if found {
                    return Err("frpc 压缩包包含重复程序。".into());
                }
                write_payload(&mut entry, MAX_EXECUTABLE, destination, cancel)?;
                found = true;
            }
        }
    }
    if !found {
        return Err("frpc 压缩包中没有找到客户端程序。".into());
    }
    Ok(())
}
fn write_payload(
    reader: &mut impl Read,
    limit: u64,
    path: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "无法解压 frpc 程序。")?;
    let mut written = 0u64;
    let mut buffer = [0; 65536];
    loop {
        check(cancel)?;
        let n = reader
            .read(&mut buffer)
            .map_err(|_| "读取 frpc 压缩条目失败。")?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > limit {
            return Err("frpc 程序解压后过大。".into());
        }
        file.write_all(&buffer[..n])
            .map_err(|_| "写入 frpc 程序失败。")?;
    }
    if written == 0 {
        return Err("frpc 程序为空。".into());
    }
    file.sync_all().map_err(|_| "保存 frpc 程序失败。".into())
}
fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("操作已取消。".into())
    } else {
        Ok(())
    }
}
struct Stage {
    directory: PathBuf,
    archive: PathBuf,
    executable: PathBuf,
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.archive);
        let _ = fs::remove_file(&self.executable);
        let _ = fs::remove_dir(&self.directory);
    }
}

#[cfg(test)]
mod tests;
