use super::*;
use crate::instances::{self, backup};

#[derive(Clone)]
pub struct PackUpdatePlan {
    pub instance: Instance,
    pub previous_version: String,
    pub next: PackPlan,
    pub changes: Vec<String>,
    pub conflicts: Vec<String>,
    pub replace_modified: bool,
    previous: Registry,
    observed: BTreeMap<String, Option<InstalledFile>>,
}
fn game_data(path: &str) -> bool {
    path.starts_with("saves/")
        || path.starts_with("screenshots/")
        || matches!(
            path,
            "options.txt"
                | "optionsof.txt"
                | "optionsshaders.txt"
                | "servers.dat"
                | "servers.dat_old"
        )
}
fn observed(directory: &Path, path: &str) -> Result<Option<InstalledFile>> {
    let p = files::safe_path(directory, path)?;
    if !p.exists() {
        Ok(None)
    } else {
        Ok(Some(files::record(&p, path.into())?))
    }
}
pub fn plan_update(
    root: &Path,
    instance: &Instance,
    next: PackPlan,
    d: &Downloader,
) -> Result<PackUpdatePlan> {
    let current = instances::list(root)
        .into_iter()
        .find(|i| i.id == instance.id)
        .ok_or("实例不存在或已归档。")?;
    if current != *instance || instance.isolation != instances::Isolation::Isolated {
        return Err("整合包升级需要未改变的独立实例。".into());
    }
    let directory = instance.game_directory(root)?;
    let previous = super::super::registry(&directory)?;
    let old = previous
        .pack
        .as_ref()
        .ok_or("此实例没有整合包记录，不能自动升级。")?;
    if old
        .origin
        .as_ref()
        .zip(next.origin.as_ref())
        .is_some_and(|(a, b)| a.provider != b.provider || a.project_id != b.project_id)
    {
        return Err("不能将一个整合包升级到其他项目，请另建实例。".into());
    }
    let target = super::super::Target {
        instance: instance.clone(),
        minecraft: next.minecraft.clone(),
        loader: next.loader.as_ref().map(|l| l.kind),
        loader_version: next.loader.as_ref().map(|l| l.version.clone()),
    };
    for (_, entry) in previous
        .mods
        .iter()
        .filter(|(id, m)| m.enabled() && !previous.pack_mods.contains(*id))
    {
        if !target.compatible(&entry.version) {
            return Err(format!(
                "玩家追加的模组 {} 不兼容新游戏/加载器版本，请先处理该模组或新建实例。",
                entry.version.name
            ));
        }
    }
    for entry in previous.resources.values() {
        if !entry.version.game_versions.contains(&next.minecraft) {
            return Err(format!(
                "追加资源 {} 不支持新游戏版本，请先处理该资源。",
                entry.version.name
            ));
        }
    }
    let old_files: BTreeMap<_, _> = old.files.iter().map(|f| (f.path.clone(), f)).collect();
    let new_paths: BTreeSet<_> = next
        .index
        .files
        .iter()
        .filter(|f| f.active(next.include_optional))
        .map(|f| f.path.clone())
        .chain(next.overrides.keys().cloned())
        .collect();
    let all: BTreeSet<_> = old_files
        .keys()
        .cloned()
        .chain(new_paths.iter().cloned())
        .collect();
    let mut plan = PackUpdatePlan {
        instance: instance.clone(),
        previous_version: old.version.clone(),
        next,
        changes: Vec::new(),
        conflicts: Vec::new(),
        replace_modified: false,
        previous: previous.clone(),
        observed: BTreeMap::new(),
    };
    for path in all {
        d.check_cancelled()?;
        let actual = observed(&directory, &path)?;
        let changed = match (old_files.get(&path), &actual) {
            (Some(old), Some(actual)) => {
                old.size != actual.size
                    || old
                        .hashes
                        .iter()
                        .any(|(k, v)| actual.hashes.get(k) != Some(v))
            }
            (None, Some(_)) => true,
            _ => false,
        };
        if changed || game_data(&path) && actual.is_some() {
            plan.conflicts.push(path.clone());
        }
        plan.changes.push(format!(
            "{} {path}",
            if !new_paths.contains(&path) {
                "移除包文件"
            } else if old_files.contains_key(&path) {
                "更新包文件"
            } else {
                "新增包文件"
            }
        ));
        plan.observed.insert(path, actual);
    }
    Ok(plan)
}

pub fn install_update(
    root: &Path,
    plan: &PackUpdatePlan,
    settings: &Settings,
    d: &Downloader,
) -> Result<Instance> {
    let current = instances::list(root)
        .into_iter()
        .find(|i| i.id == plan.instance.id)
        .ok_or("实例已归档。")?;
    let directory = current.game_directory(root)?;
    if current != plan.instance || super::super::registry(&directory)? != plan.previous {
        return Err("实例已改变，请重新预览升级方案。".into());
    }
    for (path, before) in &plan.observed {
        d.check_cancelled()?;
        if observed(&directory, path)? != *before {
            return Err(format!("文件在预览后已改变：{path}。请重新生成方案。"));
        }
    }
    let mut prepared = prepare_pack(root, &plan.next, settings, d)?;
    backup::create_for_update(
        root,
        &current,
        &backup::directory(settings),
        &plan.observed.keys().cloned().collect::<Vec<_>>(),
        d,
    )?;
    let mut updated = current.clone();
    updated.version_id = prepared.instance.version_id;
    let mut next = prepared.record;
    next.mods = plan
        .previous
        .mods
        .iter()
        .filter(|(id, _)| !plan.previous.pack_mods.contains(*id))
        .map(|(id, m)| (id.clone(), m.clone()))
        .collect();
    next.pinned = plan
        .previous
        .pinned
        .iter()
        .filter(|id| next.mods.contains_key(*id))
        .cloned()
        .collect();
    next.resources = plan.previous.resources.clone();
    // Carry disabled state across renamed mod releases using the embedded mod ID.
    let disabled: Vec<_> = plan
        .previous
        .pack
        .as_ref()
        .unwrap()
        .files
        .iter()
        .filter_map(|f| {
            f.path
                .strip_prefix(".nscl/disabled/")
                .or_else(|| {
                    f.path
                        .strip_suffix(".disabled")
                        .filter(|p| p.starts_with("mods/"))
                })
                .map(|normal| (normal.to_owned(), f))
        })
        .collect();
    let mut disabled_ids = BTreeSet::new();
    for (normal, old) in &disabled {
        if normal.starts_with("mods/")
            && let Ok(mods) =
                super::super::metadata::read(&files::safe_path(&directory, &old.path)?)
        {
            disabled_ids.extend(mods.into_iter().map(|m| m.id));
        }
    }
    let mut new_sources = BTreeMap::new();
    for file in &mut next.pack.as_mut().unwrap().files {
        let source = files::safe_path(&prepared.payload, &file.path)?;
        let keep_disabled = disabled.iter().any(|(normal, _)| normal == &file.path)
            || file.path.starts_with("mods/")
                && super::super::metadata::read(&source)
                    .is_ok_and(|mods| mods.iter().any(|m| disabled_ids.contains(&m.id)));
        if keep_disabled {
            file.path = if file.path.starts_with("mods/") {
                format!("{}.disabled", file.path)
            } else {
                format!(".nscl/disabled/{}", file.path)
            };
            if !plan.observed.contains_key(&file.path)
                && observed(&directory, &file.path)?.is_some()
            {
                return Err(format!("禁用目录存在同名文件，请先处理：{}", file.path));
            }
        }
        new_sources.insert(file.path.clone(), source);
    }
    let all_paths: BTreeSet<_> = plan
        .observed
        .keys()
        .cloned()
        .chain(new_sources.keys().cloned())
        .collect();
    let new_paths: BTreeSet<_> = next
        .pack
        .as_ref()
        .unwrap()
        .files
        .iter()
        .map(|f| f.path.clone())
        .collect();
    let mut changes = BTreeMap::new();
    let mut protected = BTreeSet::new();
    for path in &all_paths {
        let local = plan.observed.get(path).is_some_and(|f| f.is_some());
        if local && (game_data(path) || plan.conflicts.contains(path) && !plan.replace_modified) {
            continue;
        }
        if new_paths.contains(path) {
            changes.insert(path.clone(), Some(new_sources[path].clone()));
            if !local {
                protected.insert(path.clone());
            }
        } else if local {
            changes.insert(path.clone(), None);
        }
    }
    let metadata = prepared.stage.path.join("upgraded-registry.json");
    crate::store::write_json(&metadata, &next)?;
    let config = prepared.stage.path.join("upgraded-instance.json");
    crate::store::write_json(&config, &updated)?;
    changes.insert(REGISTRY.into(), Some(metadata));
    changes.insert("nscl-instance.json".into(), Some(config));
    for (path, before) in &plan.observed {
        if observed(&directory, path)? != *before {
            return Err(format!(
                "升级准备期间文件改变：{path}。已保留恢复点，请重新预览。"
            ));
        }
    }
    prepared
        .stage
        .commit_protected(&directory, changes, &protected, d)?;
    Ok(updated)
}
