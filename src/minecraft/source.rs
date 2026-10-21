use super::Result;
use reqwest::Url;
use serde::{Deserialize, Serialize};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum SourceKind {
    #[default]
    Official,
    Bmclapi,
    Minguncle,
    Custom,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Source {
    pub kind: SourceKind,
    pub custom_base: String,
    pub fallback_to_official: bool,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            kind: SourceKind::Official,
            custom_base: String::new(),
            fallback_to_official: true,
        }
    }
}

impl SourceKind {
    pub const ALL: [Self; 4] = [Self::Official, Self::Bmclapi, Self::Minguncle, Self::Custom];
    pub fn label(self) -> &'static str {
        match self {
            Self::Official => "Mojang 官方源",
            Self::Bmclapi => "BMCLAPI（bangbang93）",
            Self::Minguncle => "mirror.minguncle.cn",
            Self::Custom => "自定义镜像",
        }
    }
}

impl Source {
    pub(crate) fn official_url(&self, value: &str) -> Result<String> {
        Ok(canonical_url(&valid_url(value)?, self.base()?.as_ref()).to_string())
    }

    pub fn base(&self) -> Result<Option<Url>> {
        let value = match self.kind {
            SourceKind::Official => return Ok(None),
            SourceKind::Bmclapi => "https://bmclapi2.bangbang93.com",
            SourceKind::Minguncle => "https://mirror.minguncle.cn",
            SourceKind::Custom => self.custom_base.trim(),
        };
        let url = valid_url(value)?;
        if url.query().is_some() || url.fragment().is_some() {
            return Err("镜像根地址不能包含查询参数或片段。".into());
        }
        Ok(Some(url))
    }

    pub fn candidates(&self, value: &str) -> Result<Vec<String>> {
        let url = valid_url(value)?;
        let base = self.base()?;
        let canonical = canonical_url(&url, base.as_ref());
        let Some(base) = base else {
            return Ok(vec![canonical.to_string()]);
        };
        let prefix = match canonical.host_str().unwrap_or("") {
            "piston-meta.mojang.com"
            | "launchermeta.mojang.com"
            | "piston-data.mojang.com"
            | "launcher.mojang.com" => "",
            "libraries.minecraft.net" => "/maven",
            "resources.download.minecraft.net" => "/assets",
            _ => return Ok(vec![canonical.to_string()]),
        };
        let mirror = format!(
            "{}{}{}",
            base.as_str().trim_end_matches('/'),
            prefix,
            canonical.path()
        );
        let mut result = vec![mirror];
        if self.fallback_to_official && result[0] != canonical.as_str() {
            result.push(canonical.to_string());
        }
        Ok(result)
    }
}

pub fn valid_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| "请输入完整的 HTTP/HTTPS 地址。".to_owned())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("下载地址必须为不含账号密码的 HTTP/HTTPS 地址。".into());
    }
    Ok(url)
}

// Some mirrors rewrite all URLs inside metadata. Recover the official origin before switching sources.
fn canonical_url(url: &Url, custom: Option<&Url>) -> Url {
    let known = matches!(
        url.host_str(),
        Some("bmclapi2.bangbang93.com" | "mirror.minguncle.cn")
    );
    let custom_matches = custom.is_some_and(|base| base.origin() == url.origin());
    if !known && !custom_matches {
        return url.clone();
    }
    let mut path = url.path();
    if custom_matches {
        let prefix = custom.unwrap().path().trim_end_matches('/');
        if !prefix.is_empty() {
            path = path.strip_prefix(prefix).unwrap_or(path);
        }
    }
    let (host, path) = if let Some(path) = path.strip_prefix("/assets/") {
        ("resources.download.minecraft.net", format!("/{path}"))
    } else if let Some(path) = path.strip_prefix("/maven/") {
        ("libraries.minecraft.net", format!("/{path}"))
    } else if path.starts_with("/v1/objects/") {
        ("piston-data.mojang.com", path.to_owned())
    } else if path.starts_with("/v1/packages/") || path.starts_with("/mc/game/") {
        ("piston-meta.mojang.com", path.to_owned())
    } else {
        return url.clone();
    };
    Url::parse(&format!("https://{host}{path}")).expect("known official origin")
}
