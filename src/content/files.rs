use super::InstalledFile;
use crate::minecraft::{Downloader, Job, Result, safe_join};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub(super) const MAX_FILE: u64 = 512 * 1024 * 1024;

pub(super) fn reject_link(path: &Path) -> Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("资源目录不能包含符号链接或目录联接。".into());
            }
        }
        if metadata.file_type().is_symlink() {
            return Err("资源目录不能包含符号链接。".into());
        }
    }
    Ok(())
}

pub(super) fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = safe_join(root, relative)?;
    reject_link(root)?;
    let mut current = root.to_path_buf();
    for component in Path::new(relative).components() {
        current.push(component);
        reject_link(&current)?;
    }
    Ok(path)
}

pub(super) fn validate_hashes(hashes: &BTreeMap<String, String>) -> Result<()> {
    for (key, len) in [("sha1", 40), ("sha512", 128)] {
        if !hashes
            .get(key)
            .is_some_and(|value| value.len() == len && value.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(format!("资源缺少有效的 {key} 校验值。"));
        }
    }
    Ok(())
}

pub(super) fn download_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).map_err(|_| "资源下载地址无效。")?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err("资源地址不能包含账号、密码或片段。".into());
    }
    if cfg!(test) && url.scheme() == "http" && url.host_str() == Some("127.0.0.1") {
        return Ok(());
    }
    if url.scheme() != "https"
        || !matches!(
            url.host_str(),
            Some("cdn.modrinth.com" | "github.com" | "raw.githubusercontent.com" | "gitlab.com")
        )
    {
        return Err("资源下载仅支持 Modrinth CDN、GitHub 或 GitLab 的 HTTPS 地址。".into());
    }
    Ok(())
}

pub(super) fn record(path: &Path, relative: String) -> Result<InstalledFile> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > MAX_FILE {
        return Err("单个资源文件超过 512 MiB 限制。".into());
    }
    let (mut sha1, mut sha512) = (Sha1::new(), Sha512::new());
    let mut buffer = [0; 65536];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        sha1.update(&buffer[..read]);
        sha512.update(&buffer[..read]);
    }
    Ok(InstalledFile {
        path: relative,
        size,
        hashes: BTreeMap::from([
            ("sha1".into(), format!("{:x}", sha1.finalize())),
            ("sha512".into(), format!("{:x}", sha512.finalize())),
        ]),
    })
}

pub(super) fn verified(path: &Path, hashes: &BTreeMap<String, String>, size: u64) -> Result<bool> {
    validate_hashes(hashes)?;
    reject_link(path)?;
    if !path.is_file() || fs::metadata(path).map_err(|e| e.to_string())?.len() != size {
        return Ok(false);
    }
    let actual = record(path, String::new())?;
    Ok(["sha1", "sha512"]
        .iter()
        .all(|key| actual.hashes[*key].eq_ignore_ascii_case(&hashes[*key])))
}

pub(super) fn fetch(
    root: &Path,
    urls: &[String],
    hashes: &BTreeMap<String, String>,
    size: u64,
    filename: &str,
    downloader: &Downloader,
) -> Result<PathBuf> {
    validate_hashes(hashes)?;
    safe_join(Path::new(""), filename)?;
    if filename.contains('/') {
        return Err("资源缓存文件名不能包含目录。".into());
    }
    if size > MAX_FILE || urls.is_empty() || urls.len() > 16 {
        return Err("资源大小或下载地址数量无效。".into());
    }
    for url in urls {
        download_url(url)?;
    }
    let path = safe_path(
        root,
        &format!(
            ".nscl/content-cache/{}/{filename}",
            hashes["sha512"].to_ascii_lowercase(),
        ),
    )?;
    reject_link(&path.with_file_name(format!("{filename}.part")))?;
    downloader.check_cancelled()?;
    if verified(&path, hashes, size)? {
        return Ok(path);
    }
    let mut errors = Vec::new();
    for url in urls {
        downloader.check_cancelled()?;
        let result = downloader
            .download(&Job {
                url: url.clone(),
                path: path.clone(),
                sha1: hashes["sha1"].clone(),
                size: Some(size),
            })
            .and_then(|()| {
                if verified(&path, hashes, size)? {
                    Ok(())
                } else {
                    Err("资源 SHA-512 校验失败。".into())
                }
            });
        match result {
            Ok(()) => return Ok(path),
            Err(error) => errors.push(error),
        }
    }
    Err(format!("资源下载失败：{}", errors.join("\n")))
}

pub(super) struct Stage {
    pub path: PathBuf,
    keep: bool,
}
impl Stage {
    pub fn new(root: &Path) -> Result<Self> {
        let path = safe_path(
            root,
            &format!(".nscl/content-work/{}", uuid::Uuid::new_v4()),
        )?;
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        Ok(Self { path, keep: false })
    }
    pub fn directory(&self, relative: &str) -> Result<PathBuf> {
        let path = safe_path(&self.path, relative)?;
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        Ok(path)
    }
    /// Back up every replaced file and restore it if publishing any later file fails.
    pub fn commit(
        &mut self,
        directory: &Path,
        changes: BTreeMap<String, Option<PathBuf>>,
        downloader: &Downloader,
    ) -> Result<()> {
        self.commit_protected(directory, changes, &BTreeSet::new(), downloader)
    }

    /// New resource destinations must never replace a file that appeared while staging.
    pub fn commit_protected(
        &mut self,
        directory: &Path,
        changes: BTreeMap<String, Option<PathBuf>>,
        protected: &BTreeSet<String>,
        downloader: &Downloader,
    ) -> Result<()> {
        let backup = self.directory("backup")?;
        let mut applied: Vec<(PathBuf, PathBuf, bool, bool)> = Vec::new();
        let mut changes: Vec<_> = changes.into_iter().collect();
        changes.sort_by_key(|(path, _)| path == super::REGISTRY);
        let result = (|| {
            for (relative, source) in changes {
                downloader.check_cancelled()?;
                let destination = safe_path(directory, &relative)?;
                let old = safe_path(&backup, &relative)?;
                fs::create_dir_all(destination.parent().ok_or("资源目录无效。")?)
                    .map_err(|e| e.to_string())?;
                let had_old = destination.exists();
                if had_old && protected.contains(&relative) {
                    return Err(format!("目标位置已有同名文件，未覆盖：{relative}"));
                }
                if had_old {
                    if !destination.is_file() {
                        return Err(format!("目标不是普通文件：{relative}"));
                    }
                    fs::create_dir_all(old.parent().unwrap()).map_err(|e| e.to_string())?;
                    fs::rename(&destination, &old).map_err(|e| e.to_string())?;
                }
                applied.push((destination.clone(), old, had_old, false));
                if let Some(source) = source {
                    if protected.contains(&relative) {
                        match fs::hard_link(&source, &destination) {
                            Ok(()) => {}
                            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                                return Err(format!("目标位置已有同名文件，未覆盖：{relative}"));
                            }
                            Err(_) => {
                                // Filesystems without hard links still use create_new to prevent replacement.
                                let mut output = fs::OpenOptions::new()
                                    .write(true)
                                    .create_new(true)
                                    .open(&destination)
                                    .map_err(|e| e.to_string())?;
                                applied.last_mut().unwrap().3 = true;
                                let mut input =
                                    fs::File::open(source).map_err(|e| e.to_string())?;
                                std::io::copy(&mut input, &mut output)
                                    .map_err(|e| e.to_string())?;
                                output.sync_all().map_err(|e| e.to_string())?;
                            }
                        }
                    } else {
                        fs::rename(source, &destination).map_err(|e| e.to_string())?;
                    }
                    applied.last_mut().unwrap().3 = true;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            let mut rollback_errors = Vec::new();
            for (destination, old, had_old, installed) in applied.into_iter().rev() {
                if installed && let Err(e) = fs::remove_file(&destination) {
                    rollback_errors.push(e.to_string());
                }
                if had_old && let Err(e) = fs::rename(old, &destination) {
                    rollback_errors.push(e.to_string());
                }
            }
            if !rollback_errors.is_empty() {
                self.keep = true;
                return Err(format!(
                    "{error}；部分文件恢复失败：{}。备份保留在 {}",
                    rollback_errors.join("；"),
                    backup.display()
                ));
            }
            return Err(error);
        }
        Ok(())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.keep
            && reject_link(&self.path).is_ok()
            && let (Ok(path), Some(parent)) = (fs::canonicalize(&self.path), self.path.parent())
            && let Ok(parent) = fs::canonicalize(parent)
            && path.parent() == Some(parent.as_path())
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}
