use super::{
    Downloader, Result, VersionEntry, VersionManifest,
    download::{Job, safe_join, validate_hash, validate_id, verified, write_json},
    model::*,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstalledVersion {
    pub id: String,
    pub kind: String,
    pub java_major: u32,
    pub platform: String,
    pub metadata_sha1: String,
    #[serde(default)]
    pub loader: Option<super::LoaderVersion>,
    #[serde(default)]
    pub extra_files: Vec<Artifact>,
}

#[derive(Clone, Debug)]
pub struct Platform {
    pub os: String,
    pub arch: String,
    pub version: String,
}

impl Platform {
    pub fn current() -> Self {
        let os = match std::env::consts::OS {
            "macos" => "osx",
            name => name,
        }
        .to_owned();
        #[cfg(windows)]
        let version = hidden_command("cmd")
            .args(["/C", "ver"])
            .output()
            .ok()
            .and_then(|out| {
                regex::Regex::new(r"\d+\.\d+\.\d+(?:\.\d+)?")
                    .unwrap()
                    .find(&String::from_utf8_lossy(&out.stdout))
                    .map(|m| m.as_str().to_owned())
            })
            .unwrap_or_default();
        #[cfg(not(windows))]
        let version = Command::new("uname")
            .arg("-r")
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
            .unwrap_or_default();
        Self {
            os,
            arch: std::env::consts::ARCH.into(),
            version,
        }
    }

    pub fn key(&self) -> String {
        format!("{}-{}", self.os, self.arch)
    }
}

pub fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

pub fn rules_allow(rules: &[Rule], platform: &Platform, demo: bool) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let matches_os = rule.os.as_ref().is_none_or(|os| {
            os.name.as_ref().is_none_or(|name| name == &platform.os)
                && os.arch.as_ref().is_none_or(|arch| {
                    regex::Regex::new(arch).is_ok_and(|r| r.is_match(&platform.arch))
                })
                && os.version.as_ref().is_none_or(|version| {
                    regex::Regex::new(version).is_ok_and(|r| r.is_match(&platform.version))
                })
        });
        let matches_features = rule.features.iter().all(|(feature, &expected)| {
            let actual = match feature.as_str() {
                "is_demo_user" => demo,
                "has_custom_resolution" => true,
                _ => false,
            };
            actual == expected
        });
        if matches_os && matches_features {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

pub fn manifest(downloader: &Downloader) -> Result<VersionManifest> {
    downloader.phase("获取 Minecraft 版本列表", 0);
    let mut result: VersionManifest = downloader.json(super::source::MANIFEST_URL)?;
    if result.versions.is_empty() {
        return Err("版本列表为空。".into());
    }
    for entry in &mut result.versions {
        entry.url = downloader.source.official_url(&entry.url)?;
    }
    Ok(result)
}

pub fn installed_versions(root: &Path) -> Vec<InstalledVersion> {
    let mut installed = Vec::new();
    if let Ok(entries) = fs::read_dir(root.join("versions")) {
        for entry in entries.flatten() {
            if let Ok(bytes) = fs::read(entry.path().join("nscl-instance.json"))
                && let Ok(version) = serde_json::from_slice::<InstalledVersion>(&bytes)
                && validate_id(&version.id).is_ok()
                && entry.file_name() == version.id.as_str()
                && entry.path().join(format!("{}.json", version.id)).is_file()
            {
                installed.push(version);
            }
        }
    }
    installed.sort_by(|a, b| a.id.cmp(&b.id));
    installed
}

pub fn install(
    root: &Path,
    entry: &VersionEntry,
    downloader: &Downloader,
) -> Result<InstalledVersion> {
    validate_id(&entry.id)?;
    validate_hash(&entry.sha1)?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let version_dir = safe_join(&root.join("versions"), &entry.id)?;
    let metadata = version_dir.join(format!("{}.json", entry.id));
    downloader.phase("下载并校验版本元数据", 0);
    downloader.download(&Job {
        url: entry.url.clone(),
        path: metadata.clone(),
        sha1: entry.sha1.clone(),
        size: None,
    })?;
    let version: Version = read_json(&metadata)?;
    if version.id != entry.id || version.inherits_from.is_some() {
        return Err("版本元数据不匹配，或该版本不是独立原版客户端。".into());
    }
    if version.main_class.is_empty() {
        return Err("该旧版本未提供可用的原版启动入口。".into());
    }
    validate_id(&version.asset_index.id)?;
    let index_path = root
        .join("assets/indexes")
        .join(format!("{}.json", version.asset_index.id));
    downloader.phase("下载资源索引", 0);
    downloader.download(&artifact_job(&version.asset_index.artifact, index_path))?;
    let index: AssetIndex = read_json(
        &root
            .join("assets/indexes")
            .join(format!("{}.json", version.asset_index.id)),
    )?;
    let platform = Platform::current();
    let plan = plan(&root, &version, &index, &platform)?;
    downloader.many(&plan.jobs)?;
    downloader.phase("准备本地库与旧版资源目录", 0);
    extract_natives(
        &root,
        &version.id,
        &platform,
        &plan.natives,
        Some(downloader),
    )?;
    materialize_assets(&root, &version, &index, Some(downloader))?;
    downloader.check_cancelled()?;
    fs::create_dir_all(root.join("instances").join(&version.id)).map_err(|e| e.to_string())?;
    let installed = InstalledVersion {
        id: version.id.clone(),
        kind: version.kind.clone(),
        java_major: version.java_major(),
        platform: platform.key(),
        metadata_sha1: entry.sha1.clone(),
        loader: None,
        extra_files: Vec::new(),
    };
    // The completion marker is written last. An interrupted install is never shown as playable.
    write_json(&version_dir.join("nscl-instance.json"), &installed)?;
    Ok(installed)
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("解析 {} 失败：{e}", path.display()))
}

pub fn artifact_job(artifact: &Artifact, path: PathBuf) -> Job {
    Job {
        url: artifact.url.clone(),
        path,
        sha1: artifact.sha1.clone(),
        size: artifact.size,
    }
}

pub struct Plan {
    pub jobs: Vec<Job>,
    pub classpath: Vec<PathBuf>,
    pub natives: Vec<(PathBuf, Vec<String>)>,
}

pub fn plan(
    root: &Path,
    version: &Version,
    index: &AssetIndex,
    platform: &Platform,
) -> Result<Plan> {
    validate_id(&version.id)?;
    validate_id(&version.asset_index.id)?;
    let mut jobs = BTreeMap::<PathBuf, Job>::new();
    let mut classpath = Vec::new();
    let mut natives = Vec::new();
    let mut add = |job: Job| -> Result<()> {
        validate_hash(&job.sha1)?;
        if let Some(previous) = jobs.get(&job.path)
            && previous.sha1 != job.sha1
        {
            return Err("资源清单包含同名但内容不同的文件。".into());
        }
        jobs.insert(job.path.clone(), job);
        Ok(())
    };
    for library in &version.libraries {
        if !rules_allow(&library.rules, platform, false) {
            continue;
        }
        if let Some(artifact) = &library.downloads.artifact {
            let path = safe_join(
                &root.join("libraries"),
                artifact.path.as_deref().ok_or("库文件缺少相对路径")?,
            )?;
            classpath.push(path.clone());
            add(artifact_job(artifact, path))?;
        }
        if let Some(classifier) = library.natives.get(&platform.os) {
            let classifier =
                classifier.replace("${arch}", if platform.arch == "x86" { "32" } else { "64" });
            let artifact = library
                .downloads
                .classifiers
                .get(&classifier)
                .ok_or_else(|| format!("{} 缺少本机原生库 {classifier}", library.name))?;
            let path = safe_join(
                &root.join("libraries"),
                artifact.path.as_deref().ok_or("原生库缺少路径")?,
            )?;
            natives.push((
                path.clone(),
                library
                    .extract
                    .as_ref()
                    .map(|e| e.exclude.clone())
                    .unwrap_or_default(),
            ));
            add(artifact_job(artifact, path))?;
        }
    }
    let client = version
        .downloads
        .get("client")
        .ok_or("版本未提供原版客户端文件")?;
    let path = root
        .join("versions")
        .join(&version.id)
        .join(format!("{}.jar", version.id));
    classpath.push(path.clone());
    add(artifact_job(client, path))?;
    for (name, asset) in &index.objects {
        safe_join(Path::new("assets"), name)?;
        validate_hash(&asset.hash)?;
        let relative = format!("{}/{}", &asset.hash[..2], asset.hash);
        add(Job {
            url: format!("https://resources.download.minecraft.net/{relative}"),
            path: root.join("assets/objects").join(&relative),
            sha1: asset.hash.clone(),
            size: Some(asset.size),
        })?;
    }
    if let Some(logging) = &version.logging {
        let path = safe_join(&root.join("assets/log_configs"), &logging.client.file.id)?;
        add(artifact_job(&logging.client.file.artifact, path))?;
    }
    Ok(Plan {
        jobs: jobs.into_values().collect(),
        classpath,
        natives,
    })
}

pub fn extract_natives(
    root: &Path,
    id: &str,
    platform: &Platform,
    natives: &[(PathBuf, Vec<String>)],
    downloader: Option<&Downloader>,
) -> Result<PathBuf> {
    validate_id(id)?;
    let destination = root
        .join("versions")
        .join(id)
        .join("natives")
        .join(platform.key());
    fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    for (path, excluded) in natives {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("原生库归档无效：{e}"))?;
        let mut extracted = 0u64;
        for i in 0..archive.len() {
            if let Some(downloader) = downloader {
                downloader.check_cancelled()?;
            }
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            if entry.is_dir()
                || entry.name().starts_with("META-INF/")
                || excluded
                    .iter()
                    .any(|prefix| entry.name().starts_with(prefix))
            {
                continue;
            }
            let output = safe_join(&destination, entry.name())?;
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
                || entry.size() > 256 * 1024 * 1024
            {
                return Err("拒绝不安全的原生库归档条目。".into());
            }
            extracted += entry.size();
            if extracted > 1024 * 1024 * 1024 {
                return Err("原生库解压体积超出上限。".into());
            }
            fs::create_dir_all(output.parent().ok_or("无效原生库路径")?)
                .map_err(|e| e.to_string())?;
            let mut file = File::create(output).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry.by_ref().take(256 * 1024 * 1024 + 1), &mut file)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(destination)
}

pub fn materialize_assets(
    root: &Path,
    version: &Version,
    index: &AssetIndex,
    downloader: Option<&Downloader>,
) -> Result<()> {
    if !index.virtual_assets && !index.map_to_resources {
        return Ok(());
    }
    for (name, asset) in &index.objects {
        if let Some(downloader) = downloader {
            downloader.check_cancelled()?;
        }
        let source = root
            .join("assets/objects")
            .join(&asset.hash[..2])
            .join(&asset.hash);
        for directory in [
            index
                .virtual_assets
                .then(|| root.join("assets/virtual").join(&version.asset_index.id)),
            index
                .map_to_resources
                .then(|| root.join("instances").join(&version.id).join("resources")),
        ]
        .into_iter()
        .flatten()
        {
            let destination = safe_join(&directory, name)?;
            if !verified(&destination, &asset.hash, Some(asset.size))? {
                fs::create_dir_all(destination.parent().ok_or("无效资源路径")?)
                    .map_err(|e| e.to_string())?;
                fs::copy(&source, &destination).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
