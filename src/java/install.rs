use super::{JavaRuntime, probe};
use crate::minecraft::{Downloader, Result, safe_join};
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const MAX_UNPACKED: u64 = 1024 * 1024 * 1024;

pub fn runtime_directory() -> PathBuf {
    crate::store::settings_path().with_file_name("java")
}

#[derive(Deserialize)]
struct Asset {
    binary: Binary,
}
#[derive(Deserialize)]
struct Binary {
    architecture: String,
    os: String,
    image_type: String,
    package: Package,
}
#[derive(Clone, Deserialize)]
struct Package {
    checksum: String,
    link: String,
    size: u64,
}

fn allowed_download(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    #[cfg(test)]
    if url.scheme() == "http" && url.host_str() == Some("127.0.0.1") {
        return true;
    }
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.path().starts_with("/adoptium/")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
}
fn client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .user_agent(concat!("NSCL/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            #[cfg(test)]
            if attempt.url().scheme() == "http" && attempt.url().host_str() == Some("127.0.0.1") {
                return attempt.follow();
            }
            let url = attempt.url();
            if attempt.previous().len() < 8
                && url.scheme() == "https"
                && url.host_str().is_some_and(|h| {
                    h == "github.com"
                        || h.ends_with(".githubusercontent.com")
                        || h == "api.adoptium.net"
                })
            {
                attempt.follow()
            } else {
                attempt.error("Untrusted runtime download redirect")
            }
        }))
        .build()
        .map_err(|_| "无法连接 Java 下载服务。".into())
}

fn package(major: u32, client: &Client, api: &str) -> Result<Package> {
    if !(8..=99).contains(&major) {
        return Err("Java 主版本应在 8–99 之间。".into());
    }
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "x86",
        "aarch64" => "aarch64",
        _ => return Err("当前架构暂不支持 Java 自动下载。".into()),
    };
    let response = client
        .get(format!("{api}/v3/assets/latest/{major}/hotspot"))
        .query(&[
            ("architecture", arch),
            ("os", "windows"),
            ("image_type", "jre"),
            ("vendor", "eclipse"),
        ])
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|_| format!("无法查询 Java {major}，请检查网络或选择其他主版本。"))?;
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Java 元数据过大。".into());
    }
    let assets: Vec<Asset> = serde_json::from_slice(&bytes).map_err(|_| "Java 下载元数据无效。")?;
    let package = assets
        .into_iter()
        .find(|a| {
            a.binary.architecture == arch
                && a.binary.os == "windows"
                && a.binary.image_type == "jre"
        })
        .ok_or("该 Java 版本没有适用于当前系统的运行时。")?
        .binary
        .package;
    if package.checksum.len() != 64
        || !package.checksum.bytes().all(|b| b.is_ascii_hexdigit())
        || package.size == 0
        || package.size > MAX_ARCHIVE
        || !allowed_download(&package.link)
    {
        return Err("Java 下载地址、长度或 SHA-256 无效。".into());
    }
    Ok(package)
}

fn hash_file(path: &Path, package: &Package) -> Result<bool> {
    let Ok(mut file) = fs::File::open(path) else {
        return Ok(false);
    };
    if file.metadata().map_err(|e| e.to_string())?.len() != package.size {
        return Ok(false);
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&package.checksum))
}

fn no_link(path: &Path) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("Java 目录不能是符号链接或目录联接。".into());
            }
        }
        if meta.file_type().is_symlink() {
            return Err("Java 目录不能是符号链接。".into());
        }
    }
    Ok(())
}
fn unpack(archive: &Path, destination: &Path, downloader: &Downloader) -> Result<()> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if zip.len() > 20000 {
        return Err("Java 安装包条目过多。".into());
    }
    let mut prefix = None;
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for index in 0..zip.len() {
        downloader.check_cancelled()?;
        let mut file = zip.by_index(index).map_err(|e| e.to_string())?;
        let name = file.name().trim_end_matches('/').to_owned();
        safe_join(Path::new(""), &name)?;
        let Some((root, relative)) = name.split_once('/') else {
            if file.is_dir() {
                continue;
            }
            return Err("Java 安装包缺少根目录。".into());
        };
        if prefix.get_or_insert_with(|| root.to_owned()) != root {
            return Err("Java 安装包根目录不一致。".into());
        }
        let target = safe_join(destination, relative)?;
        if !names.insert(relative.to_ascii_lowercase()) {
            return Err("Java 安装包包含重复文件。".into());
        }
        if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err("拒绝 Java 安装包中的符号链接。".into());
        }
        if file.is_dir() {
            fs::create_dir_all(target).map_err(|e| e.to_string())?;
            continue;
        }
        total = total
            .checked_add(file.size())
            .ok_or("Java 安装包体积无效。")?;
        if total > MAX_UNPACKED {
            return Err("Java 解压体积过大。".into());
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(target)
            .map_err(|e| e.to_string())?;
        let mut copied = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            downloader.check_cancelled()?;
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            if copied > MAX_UNPACKED || copied > file.size() {
                return Err("Java 文件解压长度无效。".into());
            }
            output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        }
        if copied != file.size() {
            return Err("Java 文件解压不完整。".into());
        }
    }
    Ok(())
}

pub fn install_runtime(major: u32, downloader: &Downloader) -> Result<JavaRuntime> {
    if !cfg!(windows) {
        return Err("Java 自动安装当前支持 Windows；其他系统请手动指定 Java。".into());
    }
    install_at(
        major,
        &runtime_directory(),
        downloader,
        "https://api.adoptium.net",
        probe,
    )
}
fn install_at(
    major: u32,
    directory: &Path,
    downloader: &Downloader,
    api: &str,
    validate: impl Fn(&Path) -> Result<JavaRuntime>,
) -> Result<JavaRuntime> {
    downloader.check_cancelled()?;
    downloader.phase(&format!("查询 Eclipse Temurin Java {major}"), 0);
    let client = client()?;
    let package = package(major, &client, api)?;
    no_link(directory)?;
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let destination = directory.join(format!(
        "temurin-{major}-{}-{}",
        std::env::consts::ARCH,
        &package.checksum[..12]
    ));
    no_link(&destination)?;
    if let Ok(runtime) = validate(&destination)
        && runtime.compatible(major)
    {
        return Ok(runtime);
    }
    if destination.exists() {
        return Err("已管理的 Java 目录不完整，请先移走该目录后重新安装。".into());
    }
    let archives = directory.join(".downloads");
    no_link(&archives)?;
    fs::create_dir_all(&archives).map_err(|e| e.to_string())?;
    let archive = archives.join(format!("{}.zip", package.checksum));
    no_link(&archive)?;
    if !hash_file(&archive, &package)? {
        let temporary = archives.join(format!("{}.part", uuid::Uuid::new_v4()));
        let result: Result<()> = (|| {
            let mut response = client
                .get(&package.link)
                .send()
                .and_then(|r| r.error_for_status())
                .map_err(|_| "Java 下载失败，请检查网络后重试。")?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|e| e.to_string())?;
            let mut downloaded = 0u64;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 65536];
            let mut last = Instant::now();
            loop {
                downloader.check_cancelled()?;
                let n = response.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                downloaded += n as u64;
                if downloaded > package.size {
                    return Err("Java 下载文件长度超出声明。".into());
                }
                hash.update(&buffer[..n]);
                output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                if last.elapsed() > Duration::from_millis(100) {
                    downloader.byte_progress(
                        &format!("下载 Java {major}"),
                        downloaded,
                        package.size,
                    );
                    last = Instant::now();
                }
            }
            if downloaded != package.size
                || !format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&package.checksum)
            {
                return Err("Java SHA-256 或文件长度校验失败。".into());
            }
            output.sync_all().map_err(|e| e.to_string())?;
            drop(output);
            downloader.check_cancelled()?;
            if archive.exists() {
                fs::remove_file(&archive).map_err(|e| e.to_string())?;
            }
            fs::rename(&temporary, &archive).map_err(|e| e.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
    }
    downloader.phase(&format!("解压并验证 Java {major}"), 0);
    let stage = directory.join(format!(".install-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| {
        unpack(&archive, &stage, downloader)?;
        downloader.check_cancelled()?;
        let runtime = validate(&stage)?;
        if !runtime.compatible(major) {
            return Err("下载的 Java 版本或架构与请求不符。".into());
        }
        fs::rename(&stage, &destination).map_err(|e| e.to_string())?;
        Ok(JavaRuntime {
            path: destination.join("bin/java.exe"),
            ..runtime
        })
    })();
    if stage.exists() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

#[cfg(test)]
mod tests;
