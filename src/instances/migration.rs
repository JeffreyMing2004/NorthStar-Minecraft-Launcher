//! Preview and copy one instance and its required runtime without modifying its source.
use super::{Instance, Isolation, backup};
use crate::minecraft::{self, Downloader, Result, safe_join};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};
#[derive(Clone)]
struct File {
    from: PathBuf,
    relative: String,
    size: u64,
    modified: Option<SystemTime>,
}
#[derive(Clone)]
pub struct Plan {
    pub source: PathBuf,
    pub target: PathBuf,
    pub original: Instance,
    pub instance: Instance,
    pub runtime_files: usize,
    pub content_files: usize,
    pub bytes: u64,
    files: Vec<File>,
}
pub fn target_matches(plan: &Plan, root: &Path) -> bool {
    backup::checked(root).is_ok_and(|p| p == plan.target)
}
fn observe(path: &Path) -> Result<(u64, Option<SystemTime>)> {
    for p in path.ancestors() {
        super::reject_link(p)?;
    }
    let m = fs::metadata(path).map_err(|e| format!("无法读取 {}：{e}", path.display()))?;
    if !m.is_file() {
        return Err("迁入内容包含特殊文件。".into());
    }
    Ok((m.len(), m.modified().ok()))
}
fn scan(
    directory: &Path,
    relative: &str,
    result: &mut Vec<PathBuf>,
    depth: usize,
    d: &Downloader,
) -> Result<()> {
    d.check_cancelled()?;
    if depth > 64 || result.len() >= 200_000 {
        return Err("迁入内容层级或文件数量超限。".into());
    }
    let path = safe_join(directory, relative)?;
    for p in path.ancestors() {
        super::reject_link(p)?;
    }
    let m = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    if m.is_dir() {
        for entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry
                .file_name()
                .to_str()
                .ok_or("迁入文件名不能转换为 Unicode。")?
                .to_owned();
            scan(
                directory,
                &format!("{relative}/{name}"),
                result,
                depth + 1,
                d,
            )?;
        }
    } else if m.is_file() {
        result.push(path);
    } else {
        return Err("迁入内容包含特殊文件。".into());
    }
    Ok(())
}
pub fn inspect(source: &Path, target: &Path, instance: &Instance, d: &Downloader) -> Result<Plan> {
    let source = fs::canonicalize(backup::checked(source)?).map_err(|e| e.to_string())?;
    let target = backup::checked(target)?;
    if source.starts_with(&target) || target.starts_with(&source) {
        return Err(
            "复制迁入要求来源与目标目录分开，不能相同或互相包含。复用原目录请使用另一选项。".into(),
        );
    }
    if !super::list(&source).contains(instance) {
        return Err("来源实例已改变，请重新扫描。".into());
    }
    let original = instance.clone();
    let source_game = instance.game_directory(&source)?;
    let instance = Instance {
        id: format!(
            "{}-{}",
            if instance.requires_isolation() {
                "pack"
            } else {
                "instance"
            },
            uuid::Uuid::new_v4().simple()
        ),
        name: format!("{}（迁入）", instance.name)
            .chars()
            .take(64)
            .collect(),
        isolation: Isolation::Isolated,
        pinned: false,
        ..instance.clone()
    };
    instance.validate()?;
    let runtime = minecraft::runtime_files(&source, &instance.version_id)?;
    let mut files = Vec::new();
    for from in runtime {
        d.check_cancelled()?;
        let relative = from
            .strip_prefix(&source)
            .map_err(|_| "运行库指向来源目录之外。")?
            .to_str()
            .ok_or("运行库路径无效。")?
            .replace('\\', "/");
        let (size, modified) = observe(&from)?;
        files.push(File {
            from,
            relative,
            size,
            modified,
        });
    }
    let runtime_files = files.len();
    let mut content = Vec::new();
    for relative in backup::copy_roots(&source_game)? {
        scan(&source_game, &relative, &mut content, 0, d)?;
    }
    for from in content {
        let relative = from
            .strip_prefix(&source_game)
            .map_err(|_| "实例内容路径越界。")?
            .to_str()
            .ok_or("内容路径无效。")?
            .replace('\\', "/");
        let (size, modified) = observe(&from)?;
        files.push(File {
            from,
            relative: format!("instances/{}/{relative}", instance.id),
            size,
            modified,
        });
    }
    let mut seen = BTreeSet::new();
    if files
        .iter()
        .any(|f| !seen.insert(f.relative.to_ascii_lowercase()))
    {
        return Err("迁入内容存在重名路径。".into());
    }
    let bytes = files
        .iter()
        .try_fold(0u64, |n, f| n.checked_add(f.size).ok_or("迁入总大小溢出。"))?;
    Ok(Plan {
        source,
        target,
        original,
        instance,
        runtime_files,
        content_files: files.len() - runtime_files,
        bytes,
        files,
    })
}
fn hash(path: &Path, d: &Downloader) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut state = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        d.check_cancelled()?;
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        state.update(&buffer[..n]);
    }
    Ok(format!("{:x}", state.finalize()))
}
pub fn copy(plan: &Plan, d: &Downloader) -> Result<Instance> {
    if !super::list(&plan.source).contains(&plan.original) {
        return Err("来源实例已改变，请重新预览。".into());
    }
    plan.instance.validate()?;
    if plan.instance.directory(&plan.target)?.exists() {
        return Err("目标实例目录已存在，请重新生成方案。".into());
    }
    let mut stage = crate::content::Stage::new(&plan.target)?;
    let mut changes = BTreeMap::new();
    let mut protected = BTreeSet::new();
    let mut done = 0u64;
    for (index, file) in plan.files.iter().enumerate() {
        d.check_cancelled()?;
        if observe(&file.from)? != (file.size, file.modified) {
            return Err(format!("预览后来源文件改变：{}", file.from.display()));
        }
        let destination = safe_join(&plan.target, &file.relative)?;
        for p in destination.ancestors() {
            super::reject_link(p)?;
        }
        if destination.exists() {
            if !destination.is_file()
                || fs::metadata(&destination).map_err(|e| e.to_string())?.len() != file.size
                || hash(&destination, d)? != hash(&file.from, d)?
            {
                return Err(format!(
                    "目标存在不同内容的文件，不会覆盖：{}",
                    file.relative
                ));
            }
            done += file.size;
            continue;
        }
        let staged = stage.path.join(format!("file-{index}"));
        let mut input = fs::File::open(&file.from).map_err(|e| e.to_string())?;
        let mut output = fs::File::create(&staged).map_err(|e| e.to_string())?;
        let mut buffer = [0; 65536];
        let mut size = 0u64;
        loop {
            d.check_cancelled()?;
            let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            size += n as u64;
            if size > file.size {
                return Err("复制时来源文件增长，请退出游戏后重新预览。".into());
            }
            output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            d.byte_progress("复制迁入内容", done, plan.bytes);
        }
        output.sync_all().map_err(|e| e.to_string())?;
        if let Some(modified) = file.modified {
            output
                .set_times(fs::FileTimes::new().set_modified(modified))
                .map_err(|e| e.to_string())?;
        }
        if size != file.size || observe(&file.from)? != (file.size, file.modified) {
            return Err("复制时来源文件改变，请退出游戏后重新预览。".into());
        }
        protected.insert(file.relative.clone());
        changes.insert(file.relative.clone(), Some(staged));
    }
    for file in &plan.files {
        if observe(&file.from)? != (file.size, file.modified) {
            return Err("迁入准备期间来源文件改变，请重新预览。".into());
        }
    }
    let metadata = stage.path.join("instance.json");
    crate::store::write_json(&metadata, &plan.instance)?;
    let relative = format!("instances/{}/nscl-instance.json", plan.instance.id);
    protected.insert(relative.clone());
    changes.insert(relative, Some(metadata));
    stage.commit_protected(&plan.target, changes, &protected, d)?;
    Ok(plan.instance.clone())
}
