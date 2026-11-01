mod curseforge;
mod download;
pub use curseforge::{Provider, set_api_key, website as curseforge_website};
pub use files::provide_manual;
pub fn manual_file_hash(path: &Path) -> Result<String> {
    Ok(files::record(path, String::new())?.hashes["sha1"].clone())
}
mod assets;
mod identify;
mod metadata;
mod optifine;
pub use identify::{IdentifyPlan, adopt, identify, lock as lock_mod_versions};
pub use optifine::import_optifine;
mod export;
mod files;
pub(crate) use files::Stage;
mod local;
mod manager;
mod mods;
mod pack;
mod platform;
pub use assets::{ResourcePlan, install_resource, plan_resource};

pub use download::{download_mod, download_mod_with_cache};
pub fn manual_cached(root: &Path, version: &ContentVersion) -> Result<bool> {
    for file in &version.files {
        if files::verified(&files::cache_path(root, file)?, &file.hashes, file.size)? {
            return Ok(true);
        }
    }
    Ok(false)
}
pub use export::{PackExportOptions, export_pack};
pub use local::{
    LocalAction, LocalCatalog, LocalEntry, LocalKind, change_local, import_local, list_local,
    local_directory,
};
pub use manager::{
    ManagedCatalog, ManagedEntry, RemovalPlan, UpdateCandidate, UpdateReport, check_updates,
    list_managed, plan_removal, remove_mods,
};
pub use mods::{CompatibilityReport, ModPlan, check_instance, install_mods, plan_mod, plan_mods};
pub use pack::{
    PackPlan, PackUpdatePlan, inspect_pack, inspect_pack_with_downloader, install_pack,
    install_update, plan_pack, plan_update,
};
pub use platform::{ContentKind, ContentVersion, Modrinth, Project, SearchPage, Target};

use crate::minecraft::Result;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub pinned: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub pack_mods: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub resources: BTreeMap<String, ManagedResource>,
    #[serde(default)]
    pub mods: BTreeMap<String, ManagedMod>,
    #[serde(default)]
    pub pack: Option<PackRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedMod {
    pub version: ContentVersion,
    pub file: InstalledFile,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedResource {
    pub kind: ContentKind,
    pub version: ContentVersion,
    pub file: InstalledFile,
}
impl ManagedMod {
    pub fn enabled(&self) -> bool {
        !self.file.path.ends_with(".disabled")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledFile {
    pub path: String,
    pub hashes: BTreeMap<String, String>,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackRecord {
    #[serde(default)]
    pub origin: Option<PackOrigin>,
    pub name: String,
    pub version: String,
    pub minecraft: String,
    pub loader: Option<crate::minecraft::LoaderVersion>,
    pub files: Vec<InstalledFile>,
}

pub(crate) const REGISTRY: &str = ".nscl-content.json";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackOrigin {
    pub provider: Provider,
    pub project_id: String,
    pub version_id: String,
}

pub fn registry(directory: &Path) -> Result<Registry> {
    let path = files::safe_path(directory, REGISTRY)?;
    if !path.exists() {
        return Ok(Registry::default());
    }
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
        return Err("实例模组记录过大。".into());
    }
    let record: Registry = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("实例模组记录损坏：{e}"))?;
    for (id, entry) in &record.mods {
        if id != &entry.version.project_id || !entry.file.path.starts_with("mods/") {
            return Err("实例模组记录与文件不一致。".into());
        }
        files::safe_path(directory, &entry.file.path)?;
        files::validate_hashes(&entry.file.hashes)?;
    }
    if let Some(pack) = &record.pack {
        for file in &pack.files {
            files::safe_path(directory, &file.path)?;
            files::validate_hashes(&file.hashes)?;
        }
    }
    for entry in record.resources.values() {
        if !matches!(entry.kind, ContentKind::ResourcePack | ContentKind::Shader) {
            return Err("资源记录类型无效。".into());
        }
        let folder = if entry.kind == ContentKind::ResourcePack {
            "resourcepacks"
        } else {
            "shaderpacks"
        };
        if !(entry.file.path.starts_with(&format!("{folder}/"))
            || entry
                .file
                .path
                .starts_with(&format!(".nscl/disabled/{folder}/")))
        {
            return Err("资源记录路径无效。".into());
        }
        files::safe_path(directory, &entry.file.path)?;
        files::validate_hashes(&entry.file.hashes)?;
    }
    Ok(record)
}

#[cfg(test)]
mod manager_tests;
#[cfg(test)]
mod tests;
