use super::{
    ContentVersion, ManagedMod, Modrinth, REGISTRY, Registry, Target, files,
    mods::{current_target, dependency_issues, unmanaged},
    registry,
};
use crate::minecraft::{Downloader, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone)]
pub struct ManagedEntry {
    pub installed: ManagedMod,
    pub intact: bool,
}
#[derive(Clone)]
pub struct ManagedCatalog {
    pub target: Target,
    pub entries: Vec<ManagedEntry>,
    pub other_files: Vec<String>,
}
#[derive(Clone)]
pub struct UpdateCandidate {
    pub project_id: String,
    pub current_version: String,
    pub available: ContentVersion,
}
#[derive(Clone, Default)]
pub struct UpdateReport {
    pub available: Vec<UpdateCandidate>,
    pub warnings: Vec<String>,
}
#[derive(Clone)]
pub struct RemovalPlan {
    pub target: Target,
    pub entries: Vec<ManagedMod>,
    pub unmanaged_count: usize,
    previous: Registry,
}

pub fn list_managed(
    root: &Path,
    target: &Target,
    downloader: &Downloader,
) -> Result<ManagedCatalog> {
    let target = current_target(root, target)?;
    let directory = target.instance.game_directory(root)?;
    let record = registry(&directory)?;
    let mut entries = Vec::new();
    for entry in record.mods.values() {
        downloader.check_cancelled()?;
        let file = files::safe_path(&directory, &entry.file.path)?;
        entries.push(ManagedEntry {
            installed: entry.clone(),
            intact: files::verified(&file, &entry.file.hashes, entry.file.size)?,
        });
    }
    entries.sort_by(|a, b| {
        a.installed
            .version
            .name
            .to_lowercase()
            .cmp(&b.installed.version.name.to_lowercase())
    });
    Ok(ManagedCatalog {
        other_files: unmanaged(&directory, &record)?,
        target,
        entries,
    })
}

pub fn check_updates(catalog: &ManagedCatalog, api: &Modrinth) -> Result<UpdateReport> {
    if catalog.entries.len() > 256 {
        return Err("单次最多检查 256 个已登记模组，请按实例拆分资源。".into());
    }
    let mut report = UpdateReport::default();
    for (index, entry) in catalog.entries.iter().enumerate() {
        api.downloader.check_cancelled()?;
        let current = &entry.installed.version;
        api.downloader.phase(
            &format!(
                "检查更新 {}/{}：{}",
                index + 1,
                catalog.entries.len(),
                current.name
            ),
            0,
        );
        match api.versions(&current.project_id, Some(&catalog.target)) {
            Ok(versions) => {
                if let Some(version) = versions
                    .into_iter()
                    .filter(|v| {
                        v.version_type == "release"
                            && v.id != current.id
                            && v.date_published > current.date_published
                    })
                    .max_by(|a, b| a.date_published.cmp(&b.date_published))
                {
                    report.available.push(UpdateCandidate {
                        project_id: current.project_id.clone(),
                        current_version: current.version_number.clone(),
                        available: version,
                    });
                }
            }
            Err(error) => {
                api.downloader.check_cancelled()?;
                report.warnings.push(format!("{}：{error}", current.name));
            }
        }
    }
    Ok(report)
}

pub fn plan_removal(
    root: &Path,
    target: &Target,
    projects: &BTreeSet<String>,
) -> Result<RemovalPlan> {
    if projects.is_empty() {
        return Err("请先选择要卸载的模组。".into());
    }
    let target = current_target(root, target)?;
    let previous = registry(&target.instance.game_directory(root)?)?;
    let unmanaged_count = unmanaged(&target.instance.game_directory(root)?, &previous)?.len();
    let mut entries = Vec::new();
    for project in projects {
        entries.push(
            previous
                .mods
                .get(project)
                .ok_or("模组记录已改变，请刷新已安装列表。")?
                .clone(),
        );
    }
    let remaining = previous
        .mods
        .iter()
        .filter(|(id, _)| !projects.contains(*id))
        .map(|(id, m)| (id.clone(), m.version.clone()))
        .collect();
    let issues = dependency_issues(&remaining);
    if !issues.is_empty() {
        return Err(format!(
            "卸载会破坏其他模组的必需依赖，请同时选择依赖它的模组：\n{}",
            issues.join("\n")
        ));
    }
    Ok(RemovalPlan {
        target,
        entries,
        unmanaged_count,
        previous,
    })
}

pub fn remove_mods(
    root: &Path,
    plan: &RemovalPlan,
    downloader: &Downloader,
) -> Result<crate::instances::Instance> {
    let target = current_target(root, &plan.target)?;
    let directory = target.instance.game_directory(root)?;
    if registry(&directory)? != plan.previous {
        return Err("实例模组记录已改变，请重新生成卸载方案。".into());
    }
    let mut record = plan.previous.clone();
    let mut changes = BTreeMap::new();
    for entry in &plan.entries {
        downloader.check_cancelled()?;
        let path = files::safe_path(&directory, &entry.file.path)?;
        if path.exists() && !files::verified(&path, &entry.file.hashes, entry.file.size)? {
            return Err(format!(
                "{} 已被手动修改，保留原文件；请先移出后重新生成卸载方案。",
                entry.file.path
            ));
        }
        record.mods.remove(&entry.version.project_id);
        changes.insert(entry.file.path.clone(), None);
    }
    if let Some(pack) = &mut record.pack {
        pack.files.retain(|file| !changes.contains_key(&file.path));
    }
    let mut stage = files::Stage::new(root)?;
    let payload = stage.directory("payload")?;
    let staged_registry = files::safe_path(&payload, REGISTRY)?;
    crate::store::write_json(&staged_registry, &record)?;
    changes.insert(REGISTRY.into(), Some(staged_registry));
    downloader.phase("卸载所选模组并更新安装记录", plan.entries.len());
    stage.commit(&directory, changes, downloader)?;
    Ok(target.instance)
}
