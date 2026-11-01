//! Local snapshots of game content. Runtime files and account stores are never copied.
use super::{Instance, Isolation, reject_link};
use crate::minecraft::{Downloader, Result, safe_join};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const CONTENT: &[&str] = &[
    "mods",
    "config",
    "defaultconfigs",
    "kubejs",
    "scripts",
    "resourcepacks",
    "shaderpacks",
    "resources",
    "patchouli_books",
    "global_packs",
    "saves",
    "screenshots",
    "options.txt",
    "optionsof.txt",
    "optionsshaders.txt",
    "servers.dat",
    "servers.dat_old",
    "icon.png",
    "pack.png",
    ".nscl-content.json",
    ".nscl/disabled",
];
const MAX_ENTRIES: usize = 200_000;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Scope {
    #[default]
    Instance,
    Saves,
}
impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Instance => "实例内容",
            Self::Saves => "仅存档",
        }
    }
    fn roots(self) -> Vec<String> {
        match self {
            Self::Instance => CONTENT.iter().map(|s| (*s).into()).collect(),
            Self::Saves => vec!["saves".into()],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub created: u64,
    pub instance: Instance,
    pub scope: Scope,
    pub bytes: u64,
    pub recovery: bool,
    source: PathBuf,
    entries: Vec<Entry>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    path: String,
    size: u64,
    hash: Option<String>,
}

#[derive(Clone)]
pub struct CloneOptions {
    pub name: String,
    pub saves: bool,
    pub screenshots: bool,
    pub servers: bool,
}

pub fn directory(settings: &crate::store::Settings) -> PathBuf {
    if settings.backup_directory.trim().is_empty() {
        crate::store::settings_path().with_file_name("backups")
    } else {
        PathBuf::from(settings.backup_directory.trim())
    }
}

// Check every existing ancestor, including user-selected roots, before any traversal.
fn checked(path: &Path) -> Result<PathBuf> {
    let path = std::path::absolute(path).map_err(|e| e.to_string())?;
    for ancestor in path.ancestors() {
        reject_link(ancestor)?;
    }
    // Resolve the existing prefix too: Windows canonical paths use the \\?\ prefix.
    // Compare only normalized paths, including destinations not created yet.
    let existing = path
        .ancestors()
        .find(|p| p.exists())
        .ok_or("路径没有可用的父目录。")?;
    let prefix = fs::canonicalize(existing).map_err(io)?;
    let suffix = path.strip_prefix(existing).map_err(|e| e.to_string())?;
    Ok(if suffix.as_os_str().is_empty() {
        prefix
    } else {
        prefix.join(suffix)
    })
}
fn io(error: std::io::Error) -> String {
    error.to_string()
}
fn item(base: &Path, relative: &str) -> Result<PathBuf> {
    let path = safe_join(base, relative)?;
    checked(&path)
}

/// Only removes a generated directory whose parent and type are still unchanged.
struct Work {
    path: PathBuf,
    parent: PathBuf,
    keep: bool,
}
impl Work {
    fn new(parent: &Path) -> Result<Self> {
        let parent = checked(parent)?;
        fs::create_dir_all(&parent).map_err(io)?;
        let parent = fs::canonicalize(parent).map_err(io)?;
        let path = parent.join(format!(".nscl-work-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir(&path).map_err(io)?;
        Ok(Self {
            path,
            parent,
            keep: false,
        })
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if !self.keep
            && self.path.parent() == Some(self.parent.as_path())
            && checked(&self.path).is_ok()
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn scan(
    base: &Path,
    relative: &str,
    entries: &mut Vec<Entry>,
    depth: usize,
    d: &Downloader,
) -> Result<()> {
    d.check_cancelled()?;
    if depth > 64 || entries.len() >= MAX_ENTRIES {
        return Err("备份目录层级或文件数量超过限制。".into());
    }
    let path = item(base, relative)?;
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io(e)),
    };
    if meta.is_dir() {
        entries.push(Entry {
            path: relative.into(),
            size: 0,
            hash: None,
        });
        for entry in fs::read_dir(path).map_err(io)? {
            let name = entry
                .map_err(io)?
                .file_name()
                .into_string()
                .map_err(|_| "备份文件名必须为 UTF-8。")?;
            scan(base, &format!("{relative}/{name}"), entries, depth + 1, d)?;
        }
    } else if meta.is_file() {
        entries.push(Entry {
            path: relative.into(),
            size: meta.len(),
            hash: Some(String::new()),
        });
    } else {
        return Err(format!("不能备份特殊文件：{relative}"));
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path, expected: &Entry, d: &Downloader) -> Result<String> {
    d.check_cancelled()?;
    checked(source)?;
    checked(target)?;
    let mut input = fs::File::open(source).map_err(io)?;
    let before = input.metadata().map_err(io)?;
    if !before.is_file() || before.len() != expected.size {
        return Err(format!("文件已变化：{}", expected.path));
    }
    fs::create_dir_all(target.parent().unwrap()).map_err(io)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(io)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 128 * 1024];
    let mut size = 0u64;
    loop {
        d.check_cancelled()?;
        let n = input.read(&mut buffer).map_err(io)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > expected.size {
            return Err("复制期间文件发生变化，请退出游戏后重试。".into());
        }
        output.write_all(&buffer[..n]).map_err(io)?;
        hasher.update(&buffer[..n]);
        d.byte_progress(&format!("复制：{}", expected.path), size, expected.size);
    }
    output.sync_all().map_err(io)?;
    let after = input.metadata().map_err(io)?;
    if size != expected.size || before.modified().ok() != after.modified().ok() {
        return Err(format!("复制期间文件发生变化：{}", expected.path));
    }
    let hash = format!("{:x}", hasher.finalize());
    if let Some(expected) = &expected.hash
        && !expected.is_empty()
        && expected != &hash
    {
        return Err("备份文件校验失败，原实例未被修改。".into());
    }
    Ok(hash)
}

fn copy_entries(source: &Path, target: &Path, entries: &mut [Entry], d: &Downloader) -> Result<()> {
    d.phase("复制并校验备份内容", entries.len());
    for entry in entries {
        d.check_cancelled()?;
        let from = item(source, &entry.path)?;
        let to = item(target, &entry.path)?;
        if entry.hash.is_none() {
            fs::create_dir_all(&to).map_err(io)?;
        } else {
            entry.hash = Some(copy_file(&from, &to, entry, d)?);
        }
    }
    Ok(())
}

fn source(root: &Path, instance: &Instance) -> Result<PathBuf> {
    instance.validate()?;
    checked(root)?;
    let directory = checked(&instance.game_directory(root)?)?;
    fs::create_dir_all(&directory).map_err(io)?;
    fs::canonicalize(directory).map_err(io)
}

pub fn create(
    root: &Path,
    instance: &Instance,
    destination: &Path,
    scope: Scope,
    recovery: bool,
    d: &Downloader,
) -> Result<Snapshot> {
    let from = source(root, instance)?;
    let destination = checked(destination)?;
    // The snapshot store must never be traversed as game content.
    if scope
        .roots()
        .iter()
        .any(|p| checked(&from.join(p)).is_ok_and(|base| destination.starts_with(base)))
        || destination == from
    {
        return Err("备份位置必须位于备份内容目录之外。".into());
    }
    let mut entries = Vec::new();
    for p in scope.roots() {
        scan(&from, &p, &mut entries, 0, d)?;
    }
    let work = Work::new(&destination)?;
    copy_entries(&from, &work.path.join("data"), &mut entries, d)?;
    let snapshot = Snapshot {
        id: format!("snapshot-{}", uuid::Uuid::new_v4().simple()),
        created: crate::accounts::now(),
        instance: instance.clone(),
        scope,
        bytes: entries.iter().map(|e| e.size).sum(),
        recovery,
        source: from,
        entries,
    };
    crate::store::write_json(&work.path.join("snapshot.json"), &snapshot)?;
    d.check_cancelled()?;
    fs::rename(&work.path, destination.join(&snapshot.id)).map_err(io)?;
    Ok(snapshot)
}

fn load(destination: &Path, id: &str) -> Result<Snapshot> {
    crate::minecraft::validate_id(id)?;
    if !id.starts_with("snapshot-") {
        return Err("无效的备份标识。".into());
    }
    let p = item(destination, &format!("{id}/snapshot.json"))?;
    let file = fs::File::open(p).map_err(io)?;
    if file.metadata().map_err(io)?.len() > 32 * 1024 * 1024 {
        return Err("备份清单过大。".into());
    }
    let s: Snapshot = serde_json::from_reader(file).map_err(|_| "备份清单损坏。")?;
    s.instance.validate()?;
    if s.id != id || s.entries.len() > MAX_ENTRIES {
        return Err("备份清单不合法。".into());
    }
    let roots = s.scope.roots();
    let mut seen = BTreeSet::new();
    let mut bytes = 0u64;
    for e in &s.entries {
        item(destination, &e.path)?;
        if !roots
            .iter()
            .any(|p| e.path == *p || e.path.starts_with(&format!("{p}/")))
            || !seen.insert(e.path.to_ascii_lowercase())
            || e.hash
                .as_ref()
                .is_some_and(|h| h.len() != 64 || !h.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err("备份包含非法路径或校验信息。".into());
        }
        bytes = bytes.checked_add(e.size).ok_or("备份体积溢出。")?;
    }
    if bytes != s.bytes {
        return Err("备份体积记录不一致。".into());
    }
    Ok(s)
}

pub fn list(destination: &Path, instance: &Instance) -> Result<Vec<Snapshot>> {
    checked(destination)?;
    if !destination.exists() {
        return Ok(Vec::new());
    }
    let mut snapshots = Vec::new();
    for entry in fs::read_dir(destination).map_err(io)? {
        let entry = entry.map_err(io)?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if id.starts_with("snapshot-")
            && let Ok(snapshot) = load(destination, &id)
            && snapshot.instance.id == instance.id
        {
            snapshots.push(snapshot);
        }
    }
    snapshots.sort_by(|a, b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
    Ok(snapshots)
}

/// Verify and stage first, then preserve a recovery point and swap covered roots.
/// Cancellation is accepted until the first swap; commit/rollback always finishes.
pub fn restore(
    root: &Path,
    instance: &Instance,
    destination: &Path,
    id: &str,
    d: &Downloader,
) -> Result<()> {
    restore_inner(root, instance, destination, id, d, |_| Ok(()))
}
fn restore_inner(
    root: &Path,
    instance: &Instance,
    destination: &Path,
    id: &str,
    d: &Downloader,
    before_swap: impl Fn(&str) -> Result<()>,
) -> Result<()> {
    let snapshot = load(destination, id)?;
    let target = source(root, instance)?;
    if snapshot.instance.id != instance.id
        || snapshot.instance.version_id != instance.version_id
        || snapshot.instance.isolation != instance.isolation
        || snapshot.source != target
    {
        return Err("备份属于其他实例、版本或游戏目录，请勿跨目录恢复。".into());
    }
    let mut work = Work::new(&target)?;
    let staged = work.path.join("new");
    let mut entries = snapshot.entries.clone();
    copy_entries(
        &item(destination, &format!("{id}/data"))?,
        &staged,
        &mut entries,
        d,
    )?;
    // Check the current trees as well: rollback must not follow links.
    let mut current = Vec::new();
    for p in snapshot.scope.roots() {
        scan(&target, &p, &mut current, 0, d)?;
    }
    create(root, instance, destination, snapshot.scope, true, d)?;
    d.check_cancelled()?;
    let mut moved: Vec<(PathBuf, PathBuf, bool)> = Vec::new();
    let commit = (|| -> Result<()> {
        for p in snapshot.scope.roots() {
            before_swap(&p)?;
            let to = item(&target, &p)?;
            let old = item(&work.path.join("old"), &p)?;
            let new = item(&staged, &p)?;
            let existed = to.exists();
            if existed {
                fs::create_dir_all(old.parent().unwrap()).map_err(io)?;
                fs::rename(&to, &old).map_err(io)?;
            }
            moved.push((to.clone(), old, false));
            if new.exists() {
                fs::create_dir_all(to.parent().unwrap()).map_err(io)?;
                fs::rename(new, &to).map_err(io)?;
                moved.last_mut().unwrap().2 = true;
            }
        }
        if snapshot.scope == Scope::Instance {
            super::save(root, &snapshot.instance)?;
        }
        Ok(())
    })();
    if let Err(error) = commit {
        let mut failures = Vec::new();
        for (to, old, installed) in moved.into_iter().rev() {
            let result = (|| -> Result<()> {
                if installed {
                    let discard = work
                        .path
                        .join(format!("discard-{}", uuid::Uuid::new_v4().simple()));
                    fs::rename(&to, discard).map_err(io)?;
                }
                if old.exists() {
                    fs::rename(&old, &to).map_err(io)?;
                }
                Ok(())
            })();
            if let Err(e) = result {
                failures.push(e);
            }
        }
        if !failures.is_empty() {
            work.keep = true;
            return Err(format!(
                "恢复失败：{error}；回退未完成，原文件保留在 {}：{}",
                work.path.display(),
                failures.join("；")
            ));
        }
        return Err(format!("恢复失败，原文件已回退：{error}"));
    }
    Ok(())
}

pub fn clone_instance(
    root: &Path,
    instance: &Instance,
    options: &CloneOptions,
    d: &Downloader,
) -> Result<Instance> {
    let from = source(root, instance)?;
    let clone = Instance {
        id: format!(
            "{}-{}",
            if instance.requires_isolation() {
                "pack"
            } else {
                "instance"
            },
            uuid::Uuid::new_v4().simple()
        ),
        name: options.name.trim().into(),
        isolation: Isolation::Isolated,
        last_played: None,
        ..instance.clone()
    };
    clone.validate()?;
    let target = clone.directory(root)?;
    let work = Work::new(target.parent().unwrap())?;
    let mut entries = Vec::new();
    for p in CONTENT {
        if (!options.saves && *p == "saves")
            || (!options.screenshots && *p == "screenshots")
            || (!options.servers && p.starts_with("servers.dat"))
        {
            continue;
        }
        scan(&from, p, &mut entries, 0, d)?;
    }
    copy_entries(&from, &work.path, &mut entries, d)?;
    crate::store::write_json(&work.path.join("nscl-instance.json"), &clone)?;
    d.check_cancelled()?;
    fs::rename(&work.path, &target).map_err(io)?;
    Ok(clone)
}

#[cfg(test)]
mod tests;
