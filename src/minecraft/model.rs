use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    #[serde(rename = "releaseTime", default)]
    pub release_time: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VersionManifest {
    pub versions: Vec<VersionEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Artifact {
    pub url: String,
    pub sha1: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: BTreeMap<String, Artifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: LibraryDownloads,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub natives: BTreeMap<String, String>,
    pub extract: Option<Extract>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        rules: Vec<Rule>,
        value: ArgumentValue,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AssetIndexRef {
    pub id: String,
    #[serde(flatten)]
    pub artifact: Artifact,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JavaVersion {
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Logging {
    pub client: LogConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LogConfig {
    pub argument: String,
    pub file: LogFile,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LogFile {
    pub id: String,
    #[serde(flatten)]
    pub artifact: Artifact,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    pub downloads: BTreeMap<String, Artifact>,
    pub libraries: Vec<Library>,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndexRef,
    pub arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments")]
    pub legacy_arguments: Option<String>,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
}

impl Version {
    pub fn java_major(&self) -> u32 {
        self.java_version
            .as_ref()
            .map_or(8, |version| version.major_version)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Asset {
    pub hash: String,
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AssetIndex {
    pub objects: BTreeMap<String, Asset>,
    #[serde(default, rename = "virtual")]
    pub virtual_assets: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}
