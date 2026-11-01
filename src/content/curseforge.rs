use super::{
    ContentKind, ContentVersion, Project, SearchPage, Target, files,
    platform::{Dependency, VersionFile},
};
use crate::minecraft::{Downloader, LoaderKind, ProxyMode, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, io::Read};

// 申请成功后在此填入启动器的 CurseForge API Key。
const API_KEY: &str = "";

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    #[default]
    Modrinth,
    CurseForge,
}
impl Provider {
    pub fn available(self) -> bool {
        match self {
            Self::Modrinth => true,
            Self::CurseForge => !API_KEY.trim().is_empty(),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Modrinth => "Modrinth",
            Self::CurseForge => "CurseForge",
        }
    }
}
#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
    #[serde(default)]
    pagination: Option<Pagination>,
}
#[derive(Deserialize)]
struct Pagination {
    #[serde(rename = "totalCount")]
    total: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CfProject {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    logo: Option<Logo>,
    pub class_id: Option<u64>,
    #[serde(default)]
    pub allow_mod_distribution: Option<bool>,
}
#[derive(Deserialize)]
struct Logo {
    url: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CfFile {
    pub id: u64,
    pub mod_id: u64,
    pub display_name: String,
    pub file_name: String,
    pub file_length: u64,
    #[serde(default)]
    pub download_url: Option<String>,
    pub hashes: Vec<Hash>,
    pub game_versions: Vec<String>,
    pub file_date: String,
    pub release_type: u32,
    #[serde(default)]
    dependencies: Vec<CfDependency>,
    #[serde(default)]
    is_server_pack: bool,
}
#[derive(Clone, Deserialize)]
pub(super) struct Hash {
    value: String,
    algo: u32,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfDependency {
    mod_id: u64,
    relation_type: u32,
}
#[derive(Clone)]
pub(super) struct CurseForge {
    base: String,
    key: String,
    client: reqwest::blocking::Client,
    pub downloader: Downloader,
}
impl CurseForge {
    pub fn new(downloader: Downloader) -> Result<Self> {
        let key = API_KEY.trim();
        if key.is_empty() {
            return Err("CurseForge 内容来源暂未启用，请先使用 Modrinth。".into());
        }
        Self::configured(
            downloader,
            key.into(),
            "https://api.curseforge.com/v1".into(),
        )
    }
    #[cfg(test)]
    pub(super) fn with_base(downloader: Downloader, base: String) -> Self {
        Self::configured(downloader, "test-key-never-log".into(), base).unwrap()
    }
    fn configured(downloader: Downloader, key: String, base: String) -> Result<Self> {
        let builder = reqwest::blocking::Client::builder()
            .user_agent(concat!("NSCL/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none());
        let builder = if downloader.source.proxy_mode == ProxyMode::Direct {
            builder.no_proxy()
        } else {
            builder
        };
        Ok(Self {
            base,
            key,
            client: builder
                .build()
                .map_err(|_| "无法创建 CurseForge 客户端。")?,
            downloader,
        })
    }
    fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Envelope<T>> {
        self.downloader.check_cancelled()?;
        let response = self
            .client
            .get(format!("{}{path}", self.base))
            .query(params)
            .header("x-api-key", &self.key)
            .send()
            .map_err(|_| "CurseForge 连接失败，请检查网络或代理。")?;
        if !response.status().is_success() {
            return Err(format!(
                "CurseForge API 返回 HTTP {}；请检查 API Key、访问权限或稍后重试。",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        response
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "读取 CurseForge 响应失败。")?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("CurseForge 响应过大。".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "CurseForge 响应格式无效。".into())
    }
    pub fn project(&self, id: u64) -> Result<CfProject> {
        let p: CfProject = self.get(&format!("/mods/{id}"), &[])?.data;
        if p.id != id {
            return Err("CurseForge 项目 ID 不匹配。".into());
        }
        Ok(p)
    }
    pub fn file(&self, project: u64, file: u64) -> Result<CfFile> {
        let mut f: CfFile = self
            .get(&format!("/mods/{project}/files/{file}"), &[])?
            .data;
        if f.id != file || f.mod_id != project {
            return Err("CurseForge 文件 ID 不匹配。".into());
        }
        if self.project(project)?.allow_mod_distribution == Some(false) {
            f.download_url = None;
        }
        Ok(f)
    }
    pub fn version(&self, project: u64, file: u64) -> Result<ContentVersion> {
        let mut version = self.file(project, file)?.version()?;
        version.changelog =
            match self.get::<String>(&format!("/mods/{project}/files/{file}/changelog"), &[]) {
                Ok(changelog) => plain_changelog(&changelog.data),
                Err(error) => format!("更新日志暂不可用：{error}"),
            };
        Ok(version)
    }
    pub fn search(
        &self,
        kind: ContentKind,
        query: &str,
        offset: usize,
        target: Option<&Target>,
    ) -> Result<SearchPage> {
        let class = match kind {
            ContentKind::Mod => 6,
            ContentKind::Modpack => 4471,
            _ => return Err("此分类请使用 Modrinth。".into()),
        };
        let mut params = vec![
            ("gameId", "432".into()),
            ("classId", class.to_string()),
            ("searchFilter", query.into()),
            ("index", offset.to_string()),
            ("pageSize", "20".into()),
            ("sortField", "2".into()),
            ("sortOrder", "desc".into()),
        ];
        filters(&mut params, target);
        let data: Envelope<Vec<CfProject>> = self.get("/mods/search", &params)?;
        let hits = data
            .data
            .into_iter()
            .map(|p| Project {
                project_id: format!("cf-{}", p.id),
                title: p.name,
                description: p.summary,
                author: "CurseForge".into(),
                downloads: p.download_count,
                icon_url: p.logo.map(|l| l.url),
            })
            .collect();
        Ok(SearchPage {
            hits,
            offset,
            total_hits: data.pagination.map(|p| p.total).unwrap_or(0),
        })
    }
    pub fn versions(&self, id: u64, target: Option<&Target>) -> Result<Vec<ContentVersion>> {
        let allowed = self.project(id)?.allow_mod_distribution != Some(false);
        let mut params = vec![("pageSize", "50".into())];
        filters(&mut params, target);
        let mut result = Vec::new();
        for offset in (0..1000).step_by(50) {
            params.retain(|(k, _)| *k != "index");
            params.push(("index", offset.to_string()));
            let response: Envelope<Vec<CfFile>> =
                self.get(&format!("/mods/{id}/files"), &params)?;
            let count = response.data.len();
            for mut file in response.data {
                if !allowed {
                    file.download_url = None;
                }
                if file.mod_id == id && !file.is_server_pack {
                    let mut v = file.version()?;
                    if v.loaders.is_empty()
                        && let Some(loader) = target.and_then(|t| t.loader)
                    {
                        v.loaders.push(loader.key().into());
                    }
                    if target.is_none_or(|t| t.compatible(&v)) {
                        result.push(v);
                    }
                }
            }
            if count < 50 {
                break;
            }
        }
        result.sort_by(|a, b| {
            (a.version_type != "release")
                .cmp(&(b.version_type != "release"))
                .then(b.date_published.cmp(&a.date_published))
        });
        Ok(result)
    }
    pub fn check_project(&self, id: u64, kind: ContentKind) -> Result<()> {
        let p = self.project(id)?;
        let class = match kind {
            ContentKind::Mod => 6,
            ContentKind::Modpack => 4471,
            _ => return Err("资源分类不受支持。".into()),
        };
        if p.class_id != Some(class) {
            return Err("CurseForge 项目类型不匹配。".into());
        }
        Ok(())
    }
}
fn plain_changelog(html: &str) -> String {
    let mut result = String::new();
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                tag = true;
                result.push('\n');
            }
            '>' => tag = false,
            _ if !tag && (!c.is_control() || c == '\n' || c == '\t') => result.push(c),
            _ => {}
        }
        if result.len() >= 65536 {
            break;
        }
    }
    result
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}
fn filters(params: &mut Vec<(&str, String)>, target: Option<&Target>) {
    if let Some(t) = target {
        params.push(("gameVersion", t.minecraft.clone()));
        if let Some(l) = t.loader {
            params.push((
                "modLoaderType",
                match l {
                    LoaderKind::Forge => 1,
                    LoaderKind::Fabric => 4,
                    LoaderKind::NeoForge => 6,
                    LoaderKind::Quilt => 5,
                }
                .to_string(),
            ));
        }
    }
}
pub(super) fn project_id(id: &str) -> Result<u64> {
    id.strip_prefix("cf-")
        .and_then(|s| s.parse().ok())
        .filter(|i| *i > 0)
        .ok_or_else(|| "无效的 CurseForge 项目 ID。".into())
}
pub(super) fn version_id(id: &str) -> Result<(u64, u64)> {
    let (p, f) = id
        .strip_prefix("cf-")
        .and_then(|s| s.split_once('-'))
        .ok_or("无效的 CurseForge 发布 ID。")?;
    Ok((
        p.parse().map_err(|_| "项目 ID 无效。")?,
        f.parse().map_err(|_| "文件 ID 无效。")?,
    ))
}
impl CfFile {
    pub fn version(&self) -> Result<ContentVersion> {
        if self.is_server_pack {
            return Err("不能安装服务端整合包。".into());
        }
        let hashes: BTreeMap<String, String> = self
            .hashes
            .iter()
            .filter_map(|h| match h.algo {
                1 => Some(("sha1".into(), h.value.clone())),
                _ => None,
            })
            .collect();
        files::validate_hashes(&hashes)?;
        Ok(ContentVersion {
            id: format!("cf-{}-{}", self.mod_id, self.id),
            project_id: format!("cf-{}", self.mod_id),
            name: self.display_name.clone(),
            version_number: self.display_name.clone(),
            version_type: match self.release_type {
                1 => "release",
                2 => "beta",
                _ => "alpha",
            }
            .into(),
            date_published: self.file_date.clone(),
            game_versions: self.game_versions.clone(),
            loaders: self
                .game_versions
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .filter(|s| matches!(s.as_str(), "forge" | "fabric" | "neoforge" | "quilt"))
                .collect(),
            dependencies: self
                .dependencies
                .iter()
                .filter_map(|d| {
                    match d.relation_type {
                        3 => Some("required"),
                        5 => Some("incompatible"),
                        2 => Some("optional"),
                        _ => None,
                    }
                    .map(|kind| Dependency {
                        project_id: Some(format!("cf-{}", d.mod_id)),
                        version_id: None,
                        file_name: None,
                        dependency_type: kind.into(),
                    })
                })
                .collect(),
            files: vec![VersionFile {
                filename: self.file_name.clone(),
                url: self.download_url.clone().unwrap_or_default(),
                hashes,
                size: self.file_length,
                primary: true,
            }],
            changelog: String::new(),
        })
    }
}
pub fn website(version: &ContentVersion) -> Option<String> {
    version_id(&version.id)
        .ok()
        .map(|(p, f)| format!("https://www.curseforge.com/projects/{p}/files/{f}"))
}
