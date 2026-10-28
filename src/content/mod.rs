mod download;
mod export;
mod files;
mod local;
mod manager;
mod mods;
mod pack;
mod platform;

pub use download::download_mod;
pub use export::{PackExportOptions, export_pack};
pub use local::{
    LocalAction, LocalCatalog, LocalEntry, LocalKind, change_local, import_local, list_local,
    local_directory,
};
pub use manager::{
    ManagedCatalog, ManagedEntry, RemovalPlan, UpdateCandidate, UpdateReport, check_updates,
    list_managed, plan_removal, remove_mods,
};
pub use mods::{CompatibilityReport, ModPlan, check_instance, install_mods, plan_mod};
pub use pack::{PackPlan, inspect_pack, install_pack, plan_pack};
pub use platform::{ContentKind, ContentVersion, Modrinth, Project, SearchPage, Target};

use crate::minecraft::Result;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
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
    pub name: String,
    pub version: String,
    pub minecraft: String,
    pub loader: Option<crate::minecraft::LoaderVersion>,
    pub files: Vec<InstalledFile>,
}

pub(crate) const REGISTRY: &str = ".nscl-content.json";

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
    Ok(record)
}

#[cfg(test)]
mod manager_tests;
#[cfg(test)]
mod tests;
