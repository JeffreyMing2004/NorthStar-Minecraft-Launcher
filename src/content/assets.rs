use super::{
    ContentKind, ContentVersion, ManagedResource, Modrinth, REGISTRY, Registry, Target, files,
    mods::current_target, registry,
};
use crate::minecraft::{Downloader, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
#[derive(Clone)]
pub struct ResourcePlan {
    pub target: Target,
    pub version: ContentVersion,
    pub kind: ContentKind,
    previous: Registry,
}
pub fn plan_resource(
    root: &Path,
    target: Target,
    version: ContentVersion,
    kind: ContentKind,
    api: &Modrinth,
) -> Result<ResourcePlan> {
    if !matches!(kind, ContentKind::ResourcePack | ContentKind::Shader) {
        return Err("不是可安装的资源包或光影。".into());
    }
    let target = current_target(root, &target)?;
    if !version.game_versions.contains(&target.minecraft) {
        return Err("资源不支持此 Minecraft 版本。".into());
    }
    api.check_project(&version.project_id, kind)?;
    version.file(kind)?;
    let previous = registry(&target.instance.game_directory(root)?)?;
    Ok(ResourcePlan {
        target,
        version,
        kind,
        previous,
    })
}
pub fn install_resource(
    root: &Path,
    plan: &ResourcePlan,
    d: &Downloader,
) -> Result<crate::instances::Instance> {
    let target = current_target(root, &plan.target)?;
    let directory = target.instance.game_directory(root)?;
    if registry(&directory)? != plan.previous {
        return Err("资源记录已改变，请重新生成方案。".into());
    }
    let file = plan.version.file(plan.kind)?;
    let cached = files::fetch(
        root,
        std::slice::from_ref(&file.url),
        &file.hashes,
        file.size,
        &file.filename,
        d,
    )?;
    let mut zip = zip::ZipArchive::new(fs::File::open(&cached).map_err(|e| e.to_string())?)
        .map_err(|_| "资源不是有效 ZIP。")?;
    if plan.kind == ContentKind::ResourcePack {
        if zip.by_name("pack.mcmeta").is_err() {
            return Err("资源包缺少 pack.mcmeta。".into());
        }
    } else if !(0..zip.len()).any(|i| {
        zip.by_index(i)
            .is_ok_and(|e| e.name().starts_with("shaders/"))
    }) {
        return Err("光影包缺少 shaders/ 内容。".into());
    }
    let folder = if plan.kind == ContentKind::ResourcePack {
        "resourcepacks"
    } else {
        "shaderpacks"
    };
    let key = format!("{}:{}", plan.kind.key(), plan.version.project_id);
    let existing = plan.previous.resources.get(&key);
    if let Some(old) = existing
        && files::safe_path(&directory, &old.file.path)?.exists()
        && !files::verified(
            &files::safe_path(&directory, &old.file.path)?,
            &old.file.hashes,
            old.file.size,
        )?
    {
        return Err("原资源已被手动修改，请先备份或移走后重试。".into());
    }
    let disabled = existing.is_some_and(|r| r.file.path.starts_with(".nscl/disabled/"));
    let relative = if disabled {
        format!(".nscl/disabled/{folder}/{}", file.filename)
    } else {
        format!("{folder}/{}", file.filename)
    };
    let mut protected = BTreeSet::new();
    if existing.is_none_or(|r| r.file.path != relative) {
        if files::safe_path(&directory, &relative)?.exists() {
            return Err("存在同名的未登记资源，不会覆盖。".into());
        }
        protected.insert(relative.clone());
    }
    let mut stage = files::Stage::new(root)?;
    let staged = stage.path.join("resource.zip");
    fs::copy(&cached, &staged).map_err(|e| e.to_string())?;
    let mut record = plan.previous.clone();
    record.resources.insert(
        key,
        ManagedResource {
            kind: plan.kind,
            version: plan.version.clone(),
            file: files::record(&staged, relative.clone())?,
        },
    );
    let metadata = stage.path.join("registry.json");
    crate::store::write_json(&metadata, &record)?;
    let mut changes = BTreeMap::from([
        (relative.clone(), Some(staged)),
        (REGISTRY.into(), Some(metadata)),
    ]);
    if let Some(old) = existing
        && old.file.path != relative
    {
        changes.insert(old.file.path.clone(), None);
    }
    stage.commit_protected(&directory, changes, &protected, d)?;
    Ok(target.instance)
}
