use super::{REGISTRY, files, mods::dependency_issues, registry};
use crate::{
    instances::{self, Instance},
    minecraft::{Downloader, Result},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalKind {
    Mods,
    ResourcePacks,
    Shaders,
}

impl LocalKind {
    pub fn directory(self) -> &'static str {
        match self {
            Self::Mods => "mods",
            Self::ResourcePacks => "resourcepacks",
            Self::Shaders => "shaderpacks",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Mods => "模组",
            Self::ResourcePacks => "资源包",
            Self::Shaders => "光影",
        }
    }
    fn accepts(self, name: &str, directory: bool) -> bool {
        let lower = name.to_ascii_lowercase();
        if directory {
            self != Self::Mods
        } else if self == Self::Mods {
            lower.ends_with(".jar") || lower.ends_with(".jar.disabled")
        } else {
            lower.ends_with(".zip")
        }
    }
}

#[derive(Clone)]
pub struct LocalEntry {
    pub relative: String,
    pub filename: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub directory: bool,
    pub size: u64,
    pub project: Option<String>,
    pub missing: bool,
    pub modified: Option<SystemTime>,
}

#[derive(Clone)]
pub struct LocalCatalog {
    pub root: PathBuf,
    pub instance: Instance,
    pub kind: LocalKind,
    pub game_directory: PathBuf,
    pub entries: Vec<LocalEntry>,
    pub truncated: bool,
    pub has_trash: bool,
}

#[derive(Clone, Copy)]
pub enum LocalAction {
    Enable,
    Disable,
    Remove,
}

fn current(root: &Path, expected: &Instance) -> Result<Instance> {
    let instance = instances::list(root)
        .into_iter()
        .find(|i| i.id == expected.id)
        .ok_or("实例已被移除或归档，请刷新。")?;
    if instance.version_id != expected.version_id || instance.isolation != expected.isolation {
        return Err("实例版本或隔离模式已改变，请刷新后重试。".into());
    }
    instance.game_directory(root)?;
    Ok(instance)
}

pub fn local_directory(root: &Path, instance: &Instance, relative: &str) -> Result<PathBuf> {
    let instance = current(root, instance)?;
    files::safe_path(&instance.game_directory(root)?, relative)
}

fn short(value: &str) -> String {
    value.chars().take(256).collect()
}

fn json_metadata(path: &Path, directory: bool, filename: &str) -> Option<serde_json::Value> {
    let bytes = if directory {
        let path = files::safe_path(path, filename).ok()?;
        if fs::metadata(&path).ok()?.len() > 256 * 1024 {
            return None;
        }
        fs::read(path).ok()?
    } else {
        let file = fs::File::open(path).ok()?;
        let mut zip = zip::ZipArchive::new(file).ok()?;
        if zip.len() > 50_000 {
            return None;
        }
        let file = zip.by_name(filename).ok()?;
        if file.size() > 256 * 1024 {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(256 * 1024 + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 256 * 1024 {
            return None;
        }
        bytes
    };
    serde_json::from_slice(&bytes).ok()
}

pub fn list_local(
    root: &Path,
    instance: &Instance,
    kind: LocalKind,
    downloader: &Downloader,
) -> Result<LocalCatalog> {
    let instance = current(root, instance)?;
    let game = instance.game_directory(root)?;
    let record = registry(&game)?;
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut locations = vec![(kind.directory().to_owned(), true)];
    if kind != LocalKind::Mods {
        locations.push((format!(".nscl/disabled/{}", kind.directory()), false));
    }
    'locations: for (relative_dir, available) in locations {
        let directory = files::safe_path(&game, &relative_dir)?;
        let source = match fs::read_dir(&directory) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        for entry in source {
            downloader.check_cancelled()?;
            let entry = entry.map_err(|e| e.to_string())?;
            let Some(filename) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let relative = format!("{relative_dir}/{filename}");
            let path = files::safe_path(&game, &relative)?;
            let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if !kind.accepts(&filename, metadata.is_dir()) {
                continue;
            }
            if entries.len() == 1000 {
                truncated = true;
                break 'locations;
            }
            let managed = record.mods.iter().find(|(_, m)| m.file.path == relative);
            let (name, version) = if let Some((_, managed)) = managed {
                (
                    managed.version.name.clone(),
                    managed.version.version_number.clone(),
                )
            } else if kind == LocalKind::Mods {
                let metadata = json_metadata(&path, false, "fabric.mod.json");
                (
                    metadata
                        .as_ref()
                        .and_then(|m| m["name"].as_str())
                        .map(short)
                        .unwrap_or_else(|| filename.trim_end_matches(".disabled").to_owned()),
                    metadata
                        .as_ref()
                        .and_then(|m| m["version"].as_str())
                        .map(short)
                        .unwrap_or_default(),
                )
            } else {
                (filename.trim_end_matches(".zip").to_owned(), String::new())
            };
            entries.push(LocalEntry {
                relative,
                filename: filename.clone(),
                name,
                version,
                enabled: available
                    && !(kind == LocalKind::Mods
                        && filename.to_ascii_lowercase().ends_with(".disabled")),
                directory: metadata.is_dir(),
                size: metadata.len(),
                modified: metadata.modified().ok(),
                project: managed.map(|(id, _)| id.clone()),
                missing: false,
            });
        }
    }
    if kind == LocalKind::Mods {
        for (id, managed) in &record.mods {
            if entries.iter().any(|e| e.relative == managed.file.path) {
                continue;
            }
            if entries.len() == 1000 {
                truncated = true;
                break;
            }
            entries.push(LocalEntry {
                relative: managed.file.path.clone(),
                filename: Path::new(&managed.file.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                name: managed.version.name.clone(),
                version: managed.version.version_number.clone(),
                enabled: managed.enabled(),
                directory: false,
                size: managed.file.size,
                project: Some(id.clone()),
                missing: true,
                modified: None,
            });
        }
    }
    entries.sort_by_key(|e| (e.name.to_lowercase(), e.relative.clone()));
    let has_trash = files::safe_path(&game, ".nscl/trash")?.is_dir();
    Ok(LocalCatalog {
        root: root.to_path_buf(),
        instance,
        kind,
        game_directory: game,
        entries,
        truncated,
        has_trash,
    })
}

fn belongs(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|s| s.starts_with('/'))
}

/// Move resources rather than deleting user files. Registered changes publish with their record.
pub fn change_local(
    root: &Path,
    catalog: &LocalCatalog,
    entry: &LocalEntry,
    action: LocalAction,
    downloader: &Downloader,
) -> Result<String> {
    if root != catalog.root {
        return Err("游戏目录已改变，请刷新后重试。".into());
    }
    let instance = current(root, &catalog.instance)?;
    let game = instance.game_directory(root)?;
    let source = files::safe_path(&game, &entry.relative)?;
    if entry.missing {
        let id = entry
            .project
            .as_ref()
            .ok_or("此文件没有可移除的安装记录。")?;
        if !matches!(action, LocalAction::Remove)
            || source.exists()
            || !catalog
                .entries
                .iter()
                .any(|e| e.relative == entry.relative && e.project == entry.project)
        {
            return Err("文件状态已改变，请刷新后重试。".into());
        }
        let record = registry(&game)?;
        if !record
            .mods
            .get(id)
            .is_some_and(|m| m.file.path == entry.relative)
        {
            return Err("安装记录已改变，请刷新。".into());
        }
        let target = super::Target::from_instance(root, &instance)?;
        let plan = super::plan_removal(root, &target, &BTreeSet::from([id.clone()]))?;
        super::remove_mods(root, &plan, downloader)?;
        return Ok(format!("已移除失效安装记录：{}", entry.filename));
    }
    let metadata = fs::symlink_metadata(&source).map_err(|e| e.to_string())?;
    if metadata.is_dir() != entry.directory
        || metadata.len() != entry.size
        || metadata.modified().ok() != entry.modified
    {
        return Err("文件已改变，请刷新列表后重试。".into());
    }
    if !catalog
        .entries
        .iter()
        .any(|known| known.relative == entry.relative)
    {
        return Err("文件不属于此资源列表。".into());
    }
    let mut record = registry(&game)?;
    let managed_id = record
        .mods
        .iter()
        .find(|(_, m)| m.file.path == entry.relative)
        .map(|(id, _)| id.clone());
    if managed_id != entry.project {
        return Err("资源安装记录已改变，请刷新后重试。".into());
    }
    if let Some(id) = &managed_id {
        let managed = &record.mods[id];
        if matches!(action, LocalAction::Enable) {
            super::Target::from_instance(root, &instance)?.check(&managed.version)?;
        }
        if !files::verified(&source, &managed.file.hashes, managed.file.size)? {
            return Err("模组文件已被修改，请先备份并刷新安装记录。".into());
        }
    }
    let (destination, label) = match action {
        LocalAction::Enable if !entry.enabled => {
            let name = if catalog.kind == LocalKind::Mods {
                &entry.filename[..entry.filename.len() - ".disabled".len()]
            } else {
                &entry.filename
            };
            (format!("{}/{name}", catalog.kind.directory()), "已启用")
        }
        LocalAction::Disable if entry.enabled => {
            if catalog.kind == LocalKind::Mods {
                (format!("{}.disabled", entry.relative), "已禁用")
            } else {
                (
                    format!(
                        ".nscl/disabled/{}/{}",
                        catalog.kind.directory(),
                        entry.filename
                    ),
                    "已禁用",
                )
            }
        }
        LocalAction::Remove => (
            format!(
                ".nscl/trash/{}/{}/{}",
                catalog.kind.directory(),
                uuid::Uuid::new_v4(),
                entry.filename
            ),
            "已移到本地回收目录",
        ),
        _ => return Err("资源状态已改变，请刷新。".into()),
    };
    let target = files::safe_path(&game, &destination)?;
    if target.exists() {
        return Err("目标位置已有同名文件，已保留两份资源。".into());
    }
    if let Some(id) = &managed_id {
        let mut active: BTreeMap<_, _> = record
            .mods
            .iter()
            .filter(|(_, m)| m.enabled())
            .map(|(id, m)| (id.clone(), m.version.clone()))
            .collect();
        if matches!(action, LocalAction::Enable) {
            active.insert(id.clone(), record.mods[id].version.clone());
        } else {
            active.remove(id);
        }
        let issues = dependency_issues(&active);
        if !issues.is_empty() {
            return Err(format!(
                "操作会影响已登记模组的依赖：\n{}",
                issues.join("\n")
            ));
        }
        if matches!(action, LocalAction::Remove) {
            record.mods.remove(id);
        } else {
            record.mods.get_mut(id).unwrap().file.path = destination.clone();
        }
    }
    if let Some(pack) = &mut record.pack {
        if matches!(action, LocalAction::Remove) {
            pack.files.retain(|f| !belongs(&f.path, &entry.relative));
        } else {
            for file in &mut pack.files {
                if belongs(&file.path, &entry.relative) {
                    file.path = format!("{destination}{}", &file.path[entry.relative.len()..]);
                }
            }
        }
    }
    downloader.check_cancelled()?;
    let record_path = files::safe_path(&game, REGISTRY)?;
    if entry.directory {
        fs::create_dir_all(target.parent().ok_or("资源目录无效。")?).map_err(|e| e.to_string())?;
        fs::rename(&source, &target).map_err(|e| e.to_string())?;
        if let Err(error) = crate::store::write_json(&record_path, &record) {
            fs::rename(&target, &source)
                .map_err(|e| format!("{error}；资源保留在 {}：{e}", target.display()))?;
            return Err(error);
        }
    } else {
        let mut stage = files::Stage::new(&game)?;
        let payload = stage.path.join("resource");
        fs::copy(&source, &payload).map_err(|e| e.to_string())?;
        let staged_record = stage.path.join("record.json");
        crate::store::write_json(&staged_record, &record)?;
        stage.commit_protected(
            &game,
            BTreeMap::from([
                (entry.relative.clone(), None),
                (destination.clone(), Some(payload)),
                (REGISTRY.into(), Some(staged_record)),
            ]),
            &BTreeSet::from([destination]),
            downloader,
        )?;
    }
    Ok(format!("{label}：{}", entry.filename))
}

pub fn import_local(
    root: &Path,
    instance: &Instance,
    kind: LocalKind,
    paths: &[PathBuf],
    downloader: &Downloader,
) -> Result<String> {
    if paths.is_empty() {
        return Ok(String::new());
    }
    if paths.len() > 32 {
        return Err("每次最多导入 32 个文件。".into());
    }
    let instance = current(root, instance)?;
    let game = instance.game_directory(root)?;
    let mut stage = files::Stage::new(&game)?;
    let mut changes = BTreeMap::new();
    for (index, path) in paths.iter().enumerate() {
        downloader.check_cancelled()?;
        files::reject_link(path)?;
        let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("文件名无效。")?;
        if !metadata.is_file()
            || metadata.len() > files::MAX_FILE
            || !kind.accepts(name, false)
            || name.ends_with(".disabled")
        {
            return Err("请导入有效的 JAR 或 ZIP 文件，单个文件不超过 512 MiB。".into());
        }
        let zip = zip::ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?)
            .map_err(|_| format!("不是有效的资源压缩包：{name}"))?;
        if zip.len() > 50_000 {
            return Err("资源压缩包中的文件过多。".into());
        }
        if kind == LocalKind::ResourcePacks && !zip.file_names().any(|n| n == "pack.mcmeta") {
            return Err(format!("资源包缺少根目录 pack.mcmeta：{name}"));
        }
        if kind == LocalKind::Shaders && !zip.file_names().any(|n| n.starts_with("shaders/")) {
            return Err(format!("光影包缺少 shaders 目录：{name}"));
        }
        let relative = format!("{}/{name}", kind.directory());
        if files::safe_path(&game, &relative)?.exists()
            || changes
                .keys()
                .any(|n: &String| n.eq_ignore_ascii_case(&relative))
        {
            return Err(format!("已有同名资源，未覆盖：{name}"));
        }
        let staged = stage.path.join(format!("import-{index}"));
        fs::copy(path, &staged).map_err(|e| e.to_string())?;
        if fs::metadata(path)
            .map_err(|e| e.to_string())?
            .modified()
            .ok()
            != metadata.modified().ok()
            || fs::metadata(&staged).map_err(|e| e.to_string())?.len() != metadata.len()
        {
            return Err("导入时源文件已改变，请重试。".into());
        }
        changes.insert(relative, Some(staged));
    }
    let protected = changes.keys().cloned().collect();
    downloader.phase("发布本地资源", paths.len());
    stage.commit_protected(&game, changes, &protected, downloader)?;
    Ok(format!("已导入 {} 个{}文件", paths.len(), kind.label()))
}

#[cfg(test)]
mod tests;
