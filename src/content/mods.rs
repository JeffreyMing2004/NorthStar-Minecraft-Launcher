use super::{
    ContentKind, ContentVersion, InstalledFile, ManagedMod, Modrinth, REGISTRY, Registry, Target,
    files, platform::Dependency, registry,
};
use crate::{
    instances,
    minecraft::{Downloader, Result},
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct ModPlan {
    pub target: Target,
    pub versions: Vec<ContentVersion>,
    pub warnings: Vec<String>,
    previous: Registry,
}

#[derive(Clone, Debug, Default)]
pub struct CompatibilityReport {
    pub checked_files: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

fn matches_dependency(dep: &Dependency, versions: &BTreeMap<String, ContentVersion>) -> bool {
    versions.values().any(|v| {
        dep.project_id.as_ref().is_none_or(|p| p == &v.project_id)
            && dep.version_id.as_ref().is_none_or(|id| id == &v.id)
            && (dep.project_id.is_some() || dep.version_id.is_some())
    })
}

pub(super) fn dependency_issues(versions: &BTreeMap<String, ContentVersion>) -> Vec<String> {
    let mut issues = Vec::new();
    for version in versions.values() {
        for dep in &version.dependencies {
            let present = matches_dependency(dep, versions);
            if (dep.dependency_type == "required" && !present)
                || (dep.dependency_type == "incompatible" && present)
            {
                issues.push(format!(
                    "{} {}：{} {}",
                    version.name,
                    version.version_number,
                    if dep.dependency_type == "required" {
                        "缺少必需依赖或依赖版本不符"
                    } else {
                        "与已选或已安装模组冲突"
                    },
                    dep.version_id
                        .as_ref()
                        .or(dep.project_id.as_ref())
                        .or(dep.file_name.as_ref())
                        .map(String::as_str)
                        .unwrap_or("未知依赖")
                ));
            }
        }
    }
    issues
}

pub(super) fn unmanaged(directory: &Path, record: &Registry) -> Result<Vec<String>> {
    let mods = files::safe_path(directory, "mods")?;
    let known: BTreeSet<_> = record
        .mods
        .values()
        .map(|m| m.file.path.to_ascii_lowercase())
        .collect();
    let mut result = Vec::new();
    if mods.exists() {
        for entry in fs::read_dir(mods).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.to_ascii_lowercase().ends_with(".jar")
                && !known.contains(&format!("mods/{name}").to_ascii_lowercase())
            {
                files::safe_path(directory, &format!("mods/{name}"))?;
                result.push(name);
            }
        }
    }
    Ok(result)
}

pub(super) fn current_target(root: &Path, target: &Target) -> Result<Target> {
    let instance = instances::list(root)
        .into_iter()
        .find(|i| i.id == target.instance.id)
        .ok_or("目标实例已被移除或归档，请重新选择。")?;
    let current = Target::from_instance(root, &instance)?;
    if current.minecraft != target.minecraft
        || current.loader != target.loader
        || current.loader_version != target.loader_version
        || current.instance.version_id != target.instance.version_id
        || current.instance.isolation != target.instance.isolation
    {
        return Err("实例游戏版本、加载器或隔离模式已经改变，请重新检查安装方案。".into());
    }
    Ok(current)
}

pub fn plan_mod(
    root: &Path,
    target: Target,
    selected: ContentVersion,
    api: &Modrinth,
) -> Result<ModPlan> {
    let target = current_target(root, &target)?;
    target.check(&selected)?;
    let directory = target.instance.game_directory(root)?;
    let previous = registry(&directory)?;
    let mut chosen = BTreeMap::<String, ContentVersion>::new();
    let mut queue = VecDeque::from([selected]);
    while let Some(version) = queue.pop_front() {
        api.downloader.check_cancelled()?;
        if let Some(existing) = chosen.get(&version.project_id) {
            if existing.id != version.id {
                return Err(format!(
                    "依赖要求了同一模组的两个版本：{} {} / {}。",
                    version.name, existing.version_number, version.version_number
                ));
            }
            continue;
        }
        if chosen.len() >= 256 {
            return Err("必需依赖超过 256 个，停止解析。".into());
        }
        target.check(&version)?;
        version.file(ContentKind::Mod)?;
        api.check_project(&version.project_id, ContentKind::Mod)?;
        chosen.insert(version.project_id.clone(), version.clone());
        for dep in version
            .dependencies
            .iter()
            .filter(|d| d.dependency_type == "required")
        {
            let candidate = if let Some(id) = &dep.version_id {
                api.version(id)?
            } else if let Some(project) = &dep.project_id {
                if let Some(existing) = chosen.get(project) {
                    existing.clone()
                } else if let Some(existing) = previous
                    .mods
                    .get(project)
                    .filter(|m| target.compatible(&m.version))
                {
                    existing.version.clone()
                } else {
                    api.versions(project, Some(&target))?
                        .into_iter()
                        .next()
                        .ok_or_else(|| {
                            format!("必需依赖 {project} 没有适用于此游戏版本和加载器的版本。")
                        })?
                }
            } else {
                return Err(format!(
                    "{} 的必需依赖 {} 无法由资源接口解析。",
                    version.name,
                    dep.file_name.as_deref().unwrap_or("未知文件")
                ));
            };
            if dep
                .project_id
                .as_ref()
                .is_some_and(|id| id != &candidate.project_id)
            {
                return Err("依赖返回的项目 ID 不匹配。".into());
            }
            queue.push_back(candidate);
        }
    }
    let mut merged: BTreeMap<_, _> = previous
        .mods
        .iter()
        .map(|(id, m)| (id.clone(), m.version.clone()))
        .collect();
    merged.extend(chosen.clone());
    for version in merged.values() {
        target.check(version)?;
    }
    let issues = dependency_issues(&merged);
    if !issues.is_empty() {
        return Err(issues.join("\n"));
    }
    let mut names = BTreeMap::new();
    for (id, version) in &merged {
        let file = version.file(ContentKind::Mod)?;
        if names
            .insert(file.filename.to_ascii_lowercase(), id)
            .is_some()
        {
            return Err("多个模组使用了同一文件名，不能安全安装。".into());
        }
    }
    let mut warnings = Vec::new();
    let unknown = unmanaged(&directory, &previous)?;
    if !unknown.is_empty() {
        warnings.push(format!("实例内有 {} 个未单独登记的 JAR（含整合包自带模组），无法自动判定它们的依赖和冲突；不会覆盖同名不同内容文件。",unknown.len()));
    }
    let optional = chosen
        .values()
        .flat_map(|v| &v.dependencies)
        .filter(|d| d.dependency_type == "optional")
        .count();
    if optional > 0 {
        warnings.push(format!("另有 {optional} 项可选依赖，不会自动安装。"));
    }
    Ok(ModPlan {
        target,
        versions: chosen.into_values().collect(),
        warnings,
        previous,
    })
}

pub fn install_mods(
    root: &Path,
    plan: &ModPlan,
    downloader: &Downloader,
) -> Result<instances::Instance> {
    let target = current_target(root, &plan.target)?;
    let directory = target.instance.game_directory(root)?;
    if registry(&directory)? != plan.previous {
        return Err("实例模组记录已改变，请重新生成安装方案。".into());
    }
    let mods_directory = files::safe_path(&directory, "mods")?;
    if mods_directory.exists() {
        let names: Vec<_> = fs::read_dir(&mods_directory)
            .map_err(|e| e.to_string())?
            .map(|e| e.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect::<std::io::Result<_>>()
            .map_err(|e| e.to_string())?;
        for version in &plan.versions {
            let file = version.file(ContentKind::Mod)?;
            if names.iter().any(|name| {
                name.to_lowercase() == file.filename.to_lowercase() && name != &file.filename
            }) {
                return Err(format!(
                    "存在仅大小写不同的同名文件：{}。请先将冲突文件移出 mods 目录。",
                    file.filename
                ));
            }
        }
    }
    let mut stage = files::Stage::new(root)?;
    let payload = stage.directory("payload")?;
    let mut record = plan.previous.clone();
    let mut changes = BTreeMap::<String, Option<PathBuf>>::new();
    let mut new_files = BTreeMap::<String, InstalledFile>::new();
    for (index, version) in plan.versions.iter().enumerate() {
        target.check(version)?;
        downloader.phase(
            &format!(
                "下载模组 {}/{}：{}",
                index + 1,
                plan.versions.len(),
                version.name
            ),
            plan.versions.len(),
        );
        let file = version.file(ContentKind::Mod)?;
        let cache = files::fetch(
            root,
            std::slice::from_ref(&file.url),
            &file.hashes,
            file.size,
            &file.filename,
            downloader,
        )?;
        let relative = format!("mods/{}", file.filename);
        let staged = files::safe_path(&payload, &relative)?;
        fs::create_dir_all(staged.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(cache, &staged).map_err(|e| e.to_string())?;
        if let Some(old) = record.mods.get(&version.project_id) {
            changes.entry(old.file.path.clone()).or_insert(None);
        }
        let installed = InstalledFile {
            path: relative.clone(),
            hashes: file.hashes.clone(),
            size: file.size,
        };
        record.mods.insert(
            version.project_id.clone(),
            ManagedMod {
                version: version.clone(),
                file: installed.clone(),
            },
        );
        new_files.insert(relative.clone(), installed);
        changes.insert(relative, Some(staged));
    }
    // Recheck every existing destination before changing any file in the instance.
    for relative in changes.keys() {
        let path = files::safe_path(&directory, relative)?;
        if path.exists() {
            let known = plan
                .previous
                .mods
                .values()
                .map(|m| &m.file)
                .find(|f| f.path.eq_ignore_ascii_case(relative));
            let expected = known
                .or_else(|| new_files.get(relative))
                .ok_or("此文件没有可验证的安装记录。")?;
            if !files::verified(&path, &expected.hashes, expected.size)? {
                return Err(format!(
                    "保留已有的手动修改或同名文件：{relative}。请先将它移出 mods 目录后重试。"
                ));
            }
        }
    }
    if let Some(pack) = &mut record.pack {
        pack.files.retain(|file| !changes.contains_key(&file.path));
    }
    let staged_registry = files::safe_path(&payload, REGISTRY)?;
    crate::store::write_json(&staged_registry, &record)?;
    changes.insert(REGISTRY.into(), Some(staged_registry));
    downloader.phase("保存模组和依赖安装结果", plan.versions.len());
    stage.commit(&directory, changes, downloader)?;
    Ok(target.instance)
}

pub fn check_instance(
    root: &Path,
    instance: &instances::Instance,
    downloader: &Downloader,
) -> Result<CompatibilityReport> {
    let target = Target::from_instance(root, instance)?;
    let directory = instance.game_directory(root)?;
    let record = registry(&directory)?;
    let mut report = CompatibilityReport::default();
    let versions: BTreeMap<_, _> = record
        .mods
        .iter()
        .map(|(id, m)| (id.clone(), m.version.clone()))
        .collect();
    report.errors.extend(dependency_issues(&versions));
    for entry in record.mods.values() {
        if let Err(error) = target.check(&entry.version) {
            report.errors.push(error);
        }
    }
    let pack_files = record
        .pack
        .as_ref()
        .map(|pack| pack.files.as_slice())
        .unwrap_or_default();
    if let Some(pack) = &record.pack
        && (pack.minecraft != target.minecraft
            || pack.loader.as_ref().map(|l| l.kind) != target.loader
            || pack.loader.as_ref().map(|l| &l.version) != target.loader_version.as_ref())
    {
        report
            .errors
            .push("整合包声明的游戏版本或加载器与当前实例不一致。".into());
    }
    let all = record.mods.values().map(|m| &m.file).chain(pack_files);
    for file in all {
        downloader.check_cancelled()?;
        let path = files::safe_path(&directory, &file.path)?;
        if !files::verified(&path, &file.hashes, file.size)? {
            // Pack configuration may be edited by the player; modified JARs remain errors.
            if file.path.starts_with("mods/") {
                report
                    .errors
                    .push(format!("模组文件缺失或校验不符：{}", file.path));
            } else {
                report
                    .warnings
                    .push(format!("整合包文件已修改或移除：{}", file.path));
            }
        }
        report.checked_files += 1;
    }
    let unknown = unmanaged(&directory, &record)?;
    if !unknown.is_empty() {
        if target.loader.is_none() {
            report
                .errors
                .push("原版实例中发现模组 JAR，请先安装相应加载器。".into());
        }
        report.warnings.push(format!("{} 个 JAR 未独立登记版本依赖；整合包内文件仅依据包清单检查，手动添加的文件无法自动判断兼容性。",unknown.len()));
    }
    report.errors.truncate(100);
    report.warnings.truncate(100);
    Ok(report)
}
