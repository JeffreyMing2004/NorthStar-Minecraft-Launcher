use crate::{
    instances::Instance,
    minecraft::{self, Downloader, LoaderKind, Result},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContentKind {
    #[default]
    Mod,
    Modpack,
}
impl ContentKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Mod => "模组",
            Self::Modpack => "整合包",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Project {
    pub project_id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub downloads: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SearchPage {
    pub hits: Vec<Project>,
    pub offset: usize,
    pub total_hits: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    pub date_published: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    pub files: Vec<VersionFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub file_name: Option<String>,
    pub dependency_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionFile {
    pub filename: String,
    pub url: String,
    pub hashes: BTreeMap<String, String>,
    pub size: u64,
    #[serde(default)]
    pub primary: bool,
}
impl ContentVersion {
    pub fn file(&self, kind: ContentKind) -> Result<&VersionFile> {
        let extension = if kind == ContentKind::Mod {
            ".jar"
        } else {
            ".mrpack"
        };
        let mut files: Vec<_> = self
            .files
            .iter()
            .filter(|f| f.filename.to_ascii_lowercase().ends_with(extension))
            .collect();
        files.sort_by_key(|f| !f.primary);
        let file = files
            .first()
            .copied()
            .ok_or_else(|| format!("此版本没有可安装的 {extension} 文件。"))?;
        minecraft::safe_join(Path::new(""), &file.filename)?;
        if file.filename.contains('/') {
            return Err("资源文件名不能包含目录。".into());
        }
        super::files::validate_hashes(&file.hashes)?;
        Ok(file)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub instance: Instance,
    pub minecraft: String,
    pub loader: Option<LoaderKind>,
    pub loader_version: Option<String>,
}
impl Target {
    pub fn from_instance(root: &Path, instance: &Instance) -> Result<Self> {
        instance.validate()?;
        instance.directory(root)?;
        let installed = minecraft::installed_versions(root)
            .into_iter()
            .find(|v| v.id == instance.version_id)
            .ok_or("实例的游戏或加载器尚未完成安装。")?;
        Ok(Self {
            instance: instance.clone(),
            minecraft: installed
                .loader
                .as_ref()
                .map(|l| l.minecraft.clone())
                .unwrap_or(installed.id),
            loader_version: installed.loader.as_ref().map(|l| l.version.clone()),
            loader: installed.loader.map(|l| l.kind),
        })
    }
    pub fn label(&self) -> String {
        format!(
            "{} · Minecraft {} · {}",
            self.instance.name,
            self.minecraft,
            self.loader.map(LoaderKind::label).unwrap_or("原版")
        )
    }
    pub fn compatible(&self, version: &ContentVersion) -> bool {
        version.game_versions.contains(&self.minecraft)
            && self
                .loader
                .is_some_and(|loader| version.loaders.iter().any(|l| l == loader.key()))
    }
    pub fn check(&self, version: &ContentVersion) -> Result<()> {
        if self.compatible(version) {
            Ok(())
        } else {
            Err(format!(
                "{} {} 与目标实例不兼容：要求 Minecraft {} / {}。",
                version.name,
                version.version_number,
                self.minecraft,
                self.loader
                    .map(LoaderKind::label)
                    .unwrap_or("原版（请先安装加载器）")
            ))
        }
    }
}

#[derive(Deserialize)]
struct ProjectInfo {
    project_type: String,
    #[serde(default)]
    client_side: String,
}

#[derive(Clone)]
pub struct Modrinth {
    base: String,
    pub downloader: Downloader,
}
impl Modrinth {
    pub fn new(downloader: Downloader) -> Self {
        Self {
            base: "https://api.modrinth.com/v2".into(),
            downloader,
        }
    }

    fn url(&self, path: &str) -> Result<reqwest::Url> {
        reqwest::Url::parse(&format!("{}{path}", self.base)).map_err(|e| e.to_string())
    }

    pub fn search(
        &self,
        kind: ContentKind,
        query: &str,
        offset: usize,
        target: Option<&Target>,
    ) -> Result<SearchPage> {
        let mut facets = vec![vec![format!("project_type:{}", kind.key())]];
        if let Some(target) = target {
            facets.push(vec![format!("versions:{}", target.minecraft)]);
            if let Some(loader) = target.loader {
                facets.push(vec![format!("categories:{}", loader.key())]);
            }
        }
        let mut url = self.url("/search")?;
        url.query_pairs_mut()
            .append_pair("query", query.trim())
            .append_pair(
                "facets",
                &serde_json::to_string(&facets).map_err(|e| e.to_string())?,
            )
            .append_pair("limit", "20")
            .append_pair("offset", &offset.to_string());
        self.downloader.json(url.as_str())
    }

    pub fn versions(&self, project: &str, target: Option<&Target>) -> Result<Vec<ContentVersion>> {
        minecraft::validate_id(project)?;
        let mut url = self.url(&format!("/project/{project}/version"))?;
        if let Some(target) = target {
            url.query_pairs_mut().append_pair(
                "game_versions",
                &serde_json::json!([target.minecraft]).to_string(),
            );
            if let Some(loader) = target.loader {
                url.query_pairs_mut()
                    .append_pair("loaders", &serde_json::json!([loader.key()]).to_string());
            }
        }
        let mut versions: Vec<ContentVersion> = self.downloader.json(url.as_str())?;
        versions.retain(|v| v.project_id == project && target.is_none_or(|t| t.compatible(v)));
        versions.sort_by(|a, b| {
            (a.version_type != "release")
                .cmp(&(b.version_type != "release"))
                .then(b.date_published.cmp(&a.date_published))
        });
        Ok(versions)
    }

    pub fn version(&self, id: &str) -> Result<ContentVersion> {
        minecraft::validate_id(id)?;
        let version: ContentVersion = self
            .downloader
            .json(self.url(&format!("/version/{id}"))?.as_str())?;
        if version.id != id {
            return Err("资源接口返回的版本 ID 不匹配。".into());
        }
        Ok(version)
    }

    pub fn check_project(&self, id: &str, kind: ContentKind) -> Result<()> {
        minecraft::validate_id(id)?;
        let info: ProjectInfo = self
            .downloader
            .json(self.url(&format!("/project/{id}"))?.as_str())?;
        if info.project_type != kind.key() {
            return Err("资源类型与安装操作不一致。".into());
        }
        if info.client_side == "unsupported" {
            return Err("此资源仅支持服务端，不能安装到客户端实例。".into());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn with_base(downloader: Downloader, base: String) -> Self {
        Self { downloader, base }
    }
}
