use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
pub struct Identified {
    pub version: ContentVersion,
    pub file: InstalledFile,
    pub bundled: bool,
}
#[derive(Clone)]
pub struct IdentifyPlan {
    pub target: Target,
    pub candidates: Vec<Identified>,
    pub warnings: Vec<String>,
    previous: Registry,
}
pub fn identify(root: &Path, target: &Target, api: &Modrinth) -> Result<IdentifyPlan> {
    let target = mods::current_target(root, target)?;
    let directory = target.instance.game_directory(root)?;
    let previous = registry(&directory)?;
    let catalog = list_local(root, &target.instance, LocalKind::Mods, &api.downloader)?;
    let mut by_hash = BTreeMap::new();
    let mut warnings = Vec::new();
    for entry in catalog
        .entries
        .into_iter()
        .filter(|e| e.project.is_none() && !e.missing && !e.directory)
        .take(256)
    {
        api.downloader.check_cancelled()?;
        let file = files::record(
            &files::safe_path(&directory, &entry.relative)?,
            entry.relative.clone(),
        )?;
        by_hash
            .entry(file.hashes["sha1"].clone())
            .or_insert_with(Vec::new)
            .push(file);
    }
    let mut candidates = Vec::new();
    let mut projects = BTreeSet::new();
    for batch in by_hash.keys().cloned().collect::<Vec<_>>().chunks(100) {
        let matches = api.versions_from_hashes(batch)?;
        for hash in batch {
            let Some(version) = matches.get(hash) else {
                for file in &by_hash[hash] {
                    warnings.push(format!("未识别：{}", file.path));
                }
                continue;
            };
            if !target.compatible(version) {
                warnings.push(format!(
                    "{} 与当前游戏/加载器不兼容，未登记。",
                    version.name
                ));
                continue;
            }
            api.check_project(&version.project_id, ContentKind::Mod)?;
            for file in &by_hash[hash] {
                if previous.mods.contains_key(&version.project_id)
                    || !projects.insert(version.project_id.clone())
                {
                    return Err(format!(
                        "项目 {} 对应多个本地文件，请先处理重复模组。",
                        version.name
                    ));
                }
                let bundled = previous
                    .pack
                    .as_ref()
                    .is_some_and(|p| p.files.iter().any(|f| f.path == file.path));
                candidates.push(Identified {
                    version: version.clone(),
                    file: file.clone(),
                    bundled,
                });
            }
        }
    }
    Ok(IdentifyPlan {
        target,
        candidates,
        warnings,
        previous,
    })
}
pub fn adopt(
    root: &Path,
    plan: &IdentifyPlan,
    d: &crate::minecraft::Downloader,
) -> Result<crate::instances::Instance> {
    let target = mods::current_target(root, &plan.target)?;
    let directory = target.instance.game_directory(root)?;
    if registry(&directory)? != plan.previous {
        return Err("登记记录已改变，请重新识别。".into());
    }
    let mut record = plan.previous.clone();
    for item in &plan.candidates {
        d.check_cancelled()?;
        if !files::verified(
            &files::safe_path(&directory, &item.file.path)?,
            &item.file.hashes,
            item.file.size,
        )? {
            return Err("识别后文件已改变，请重新识别。".into());
        }
        record.mods.insert(
            item.version.project_id.clone(),
            ManagedMod {
                version: item.version.clone(),
                file: item.file.clone(),
            },
        );
        if item.bundled {
            record.pack_mods.insert(item.version.project_id.clone());
        }
    }
    d.check_cancelled()?;
    crate::store::write_json(&directory.join(REGISTRY), &record)?;
    Ok(target.instance)
}
pub fn lock(root: &Path, target: &Target, ids: &BTreeSet<String>, locked: bool) -> Result<()> {
    let target = mods::current_target(root, target)?;
    let directory = target.instance.game_directory(root)?;
    let mut record = registry(&directory)?;
    if ids.iter().any(|id| !record.mods.contains_key(id)) {
        return Err("所选模组记录已改变，请刷新。".into());
    }
    for id in ids {
        if locked {
            record.pinned.insert(id.clone());
        } else {
            record.pinned.remove(id);
        }
    }
    crate::store::write_json(&directory.join(REGISTRY), &record)
}
