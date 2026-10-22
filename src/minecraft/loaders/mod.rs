mod catalog;
mod processors;
mod profile;
pub(crate) use profile::{library_key, maven_path};

use super::{
    Downloader, InstalledVersion, Result, VersionEntry,
    download::{safe_join, validate_id, write_json},
    install::{Platform, artifact_job, extract_natives, materialize_assets, plan, read_json},
    launch::absolute_root,
    model::{Artifact, AssetIndex, Version},
};
pub use catalog::loader_versions;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{fs, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Forge,
    Fabric,
    NeoForge,
}
impl LoaderKind {
    pub const ALL: [Self; 3] = [Self::Forge, Self::Fabric, Self::NeoForge];
    pub fn label(self) -> &'static str {
        match self {
            Self::Forge => "Forge",
            Self::Fabric => "Fabric",
            Self::NeoForge => "NeoForge",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Forge => "forge",
            Self::Fabric => "fabric",
            Self::NeoForge => "neoforge",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.key() == value.to_ascii_lowercase())
            .ok_or_else(|| "加载器应为 forge、fabric 或 neoforge。".into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderVersion {
    pub kind: LoaderKind,
    pub minecraft: String,
    pub version: String,
    #[serde(default)]
    pub stable: bool,
}
impl LoaderVersion {
    pub fn id(&self) -> Result<String> {
        validate_id(&self.minecraft)?;
        validate_id(&self.version)?;
        let id = format!("{}-{}-{}", self.minecraft, self.kind.key(), self.version);
        validate_id(&id)?;
        Ok(id)
    }
    pub fn label(&self) -> String {
        format!(
            "Minecraft {} · {} {}",
            self.minecraft,
            self.kind.label(),
            self.version
        )
    }
    fn installer_url(&self) -> Result<String> {
        self.id()?;
        let (base, group, name, version) = match self.kind {
            LoaderKind::Forge => (
                "https://maven.minecraftforge.net",
                "net/minecraftforge",
                "forge",
                format!("{}-{}", self.minecraft, self.version),
            ),
            LoaderKind::NeoForge if self.minecraft == "1.20.1" => (
                "https://maven.neoforged.net/releases",
                "net/neoforged",
                "forge",
                format!("{}-{}", self.minecraft, self.version),
            ),
            LoaderKind::NeoForge => (
                "https://maven.neoforged.net/releases",
                "net/neoforged",
                "neoforge",
                self.version.clone(),
            ),
            LoaderKind::Fabric => return Err("Fabric 不使用 Forge 安装包。".into()),
        };
        Ok(format!(
            "{base}/{group}/{name}/{version}/{name}-{version}-installer.jar"
        ))
    }
}

pub fn install_loader(
    root: &Path,
    entry: &VersionEntry,
    selection: &LoaderVersion,
    java_path: &str,
    downloader: &Downloader,
) -> Result<InstalledVersion> {
    let id = selection.id()?;
    if selection.minecraft != entry.id {
        return Err("加载器与所选 Minecraft 版本不匹配。".into());
    }
    downloader.check_cancelled()?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let root = absolute_root(root)?;
    // Fetch remote loader metadata before a potentially lengthy vanilla cache check.
    // Invalid or unavailable installers fail before touching the existing profile.
    let installer = if selection.kind == LoaderKind::Fabric {
        None
    } else {
        Some(processors::download_installer(
            &root, selection, downloader,
        )?)
    };
    let fabric_profile = if selection.kind == LoaderKind::Fabric {
        let url = format!(
            "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
            entry.id, selection.version
        );
        Some(downloader.json(&url)?)
    } else {
        None
    };
    // All assets and vanilla libraries remain in the shared cache.
    super::install(&root, entry, downloader)?;
    let directory = safe_join(&root.join("versions"), &id)?;
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    fs::write(directory.join(".nscl-installing"), b"NSCL").map_err(|e| e.to_string())?;
    let marker = directory.join("nscl-instance.json");
    if marker.exists() {
        fs::remove_file(&marker).map_err(|e| e.to_string())?;
    }
    let base: Version = read_json(
        &root
            .join("versions")
            .join(&entry.id)
            .join(format!("{}.json", entry.id)),
    )?;
    downloader.phase(
        &format!("准备 {} {}", selection.kind.label(), selection.version),
        0,
    );
    let (mut child, extra_files) = match selection.kind {
        LoaderKind::Fabric => (fabric_profile.ok_or("Fabric 启动配置缺失。")?, Vec::new()),
        _ => processors::install(
            &root,
            &base,
            selection,
            java_path,
            downloader,
            installer.as_deref().ok_or("加载器安装包缺失。")?,
        )?,
    };
    if child["inheritsFrom"].as_str() != Some(entry.id.as_str()) {
        return Err("加载器安装包声明的 Minecraft 版本不匹配。".into());
    }
    profile::normalize_libraries(&root, &mut child, selection.kind, downloader)?;
    let version = profile::merge(&base, child, &id)?;
    let index: AssetIndex = read_json(
        &root
            .join("assets/indexes")
            .join(format!("{}.json", version.asset_index.id)),
    )?;
    let platform = Platform::current();
    // Keep the launch jar named after this profile. Forge excludes it by version_name.
    // Copy only when missing/damaged; assets and the much larger library cache are shared.
    let client = version
        .downloads
        .get("client")
        .ok_or("原版客户端元数据缺失。")?;
    let target = directory.join(format!("{id}.jar"));
    if !super::download::verified(&target, &client.sha1, client.size)? {
        fs::copy(
            root.join("versions")
                .join(&entry.id)
                .join(format!("{}.jar", entry.id)),
            &target,
        )
        .map_err(|e| format!("准备启动 jar 失败：{e}"))?;
    }
    let files = plan(&root, &version, &index, &platform)?;
    let base_files = plan(&root, &base, &index, &platform)?;
    let checked: std::collections::BTreeMap<_, _> = base_files
        .jobs
        .iter()
        .map(|job| (&job.path, &job.sha1))
        .collect();
    let additions: Vec<_> = files
        .jobs
        .iter()
        .filter(|job| checked.get(&job.path).copied() != Some(&job.sha1))
        .cloned()
        .collect();
    downloader.many(&additions)?;
    extract_natives(&root, &id, &platform, &files.natives, Some(downloader))?;
    materialize_assets(&root, &version, &index, Some(downloader))?;
    downloader.check_cancelled()?;
    let metadata = directory.join(format!("{id}.json"));
    write_json(&metadata, &version)?;
    let installed = InstalledVersion {
        external: false,
        id,
        kind: selection.kind.key().into(),
        java_major: version.java_major(),
        platform: platform.key(),
        metadata_sha1: format!(
            "{:x}",
            Sha1::digest(fs::read(&metadata).map_err(|e| e.to_string())?)
        ),
        loader: Some(selection.clone()),
        extra_files,
    };
    downloader.check_cancelled()?;
    // Publish only after processors, downloaded libraries, and generated files succeeded.
    write_json(&marker, &installed)?;
    let _ = fs::remove_file(directory.join(".nscl-installing"));
    Ok(installed)
}

#[cfg(test)]
mod tests;
