use super::super::{
    download::{Job, validate_hash, verified},
    install::hidden_command,
    model::Library,
};
use super::profile::{maven_path, normalize_libraries};
use super::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::Read,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};
use zip::ZipArchive;

#[derive(Deserialize)]
struct Processor {
    jar: String,
    #[serde(default)]
    sides: Option<Vec<String>>,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    outputs: BTreeMap<String, String>,
}

pub(super) fn download_installer(
    root: &Path,
    selection: &LoaderVersion,
    downloader: &Downloader,
) -> Result<PathBuf> {
    let id = selection.id()?;
    let installer_url = selection.installer_url()?;
    let installer = root.join(".nscl/installers").join(format!("{id}.jar"));
    downloader.phase("下载并校验官方加载器安装包", 0);
    downloader.download(&Job {
        sha1: downloader.checksum(&format!("{installer_url}.sha1"))?,
        url: installer_url,
        path: installer.clone(),
        size: None,
    })?;
    Ok(installer)
}

pub(super) fn install(
    root: &Path,
    base: &Version,
    selection: &LoaderVersion,
    java_path: &str,
    downloader: &Downloader,
    installer: &Path,
) -> Result<(Value, Vec<Artifact>)> {
    let id = selection.id()?;
    let mut archive = ZipArchive::new(File::open(installer).map_err(|e| e.to_string())?)
        .map_err(|e| format!("加载器安装包无效：{e}"))?;
    let profile: Value = serde_json::from_slice(&entry(
        &mut archive,
        "install_profile.json",
        16 * 1024 * 1024,
    )?)
    .map_err(|e| e.to_string())?;
    let minecraft = profile["minecraft"]
        .as_str()
        .or_else(|| profile["install"]["minecraft"].as_str());
    if minecraft != Some(base.id.as_str()) {
        return Err("加载器安装包声明的 Minecraft 版本不匹配。".into());
    }
    let child = if profile["versionInfo"].is_object() {
        profile["versionInfo"].clone()
    } else {
        let file = profile["json"].as_str().unwrap_or("/version.json");
        let file = file.trim_start_matches('/');
        safe_join(Path::new(""), file)?;
        serde_json::from_slice(&entry(&mut archive, file, 16 * 1024 * 1024)?)
            .map_err(|e| e.to_string())?
    };
    if child["inheritsFrom"].as_str() != Some(base.id.as_str()) {
        return Err("加载器启动配置的父版本不匹配。".into());
    }
    let mut tracked = BTreeSet::new();
    // Only embedded Maven artifacts are extracted, never arbitrary installer paths.
    for index in 0..archive.len() {
        downloader.check_cancelled()?;
        let file = archive.by_index(index).map_err(|e| e.to_string())?;
        let name = file.name().to_owned();
        if file.is_dir() || !name.starts_with("maven/") {
            continue;
        }
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("拒绝安装包中的符号链接。".into());
        }
        let relative = name.strip_prefix("maven/").unwrap();
        let path = safe_join(&root.join("libraries"), relative)?;
        if file.size() > 256 * 1024 * 1024 {
            return Err("安装包内嵌库过大。".into());
        }
        drop(file);
        extract(&mut archive, &name, &path)?;
        tracked.insert(path);
    }
    // The oldest installers store the universal jar outside maven/.
    if let (Some(file), Some(coordinate)) = (
        profile["install"]["filePath"].as_str(),
        profile["install"]["path"].as_str(),
    ) {
        safe_join(Path::new(""), file)?;
        let path = safe_join(&root.join("libraries"), &maven_path(coordinate)?)?;
        extract(&mut archive, file, &path)?;
        tracked.insert(path);
    }
    let processors: Vec<Processor> = serde_json::from_value(
        profile["processors"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into(),
    )
    .map_err(|e| e.to_string())?;
    let processors: Vec<_> = processors
        .into_iter()
        .filter(|processor| {
            processor
                .sides
                .as_ref()
                .is_none_or(|sides| sides.iter().any(|side| side == "client"))
        })
        .collect();
    if !processors.is_empty() {
        let java = crate::java::find(java_path, base.java_major())?;
        let mut libraries =
            json!({"libraries":profile["libraries"].as_array().cloned().unwrap_or_default()});
        normalize_libraries(root, &mut libraries, selection.kind, downloader)?;
        let libraries: Vec<Library> =
            serde_json::from_value(libraries["libraries"].clone()).map_err(|e| e.to_string())?;
        let mut jobs = Vec::new();
        for library in libraries {
            if let Some(artifact) = library.downloads.artifact {
                let path = safe_join(
                    &root.join("libraries"),
                    artifact.path.as_deref().ok_or("安装器库缺少路径。")?,
                )?;
                tracked.insert(path.clone());
                jobs.push(artifact_job(&artifact, path));
            }
        }
        downloader.many(&jobs)?;
        let work = WorkDirectory::new(root)?;
        let mut values = BTreeMap::from([
            ("SIDE".into(), "client".into()),
            ("ROOT".into(), root.to_string_lossy().into_owned()),
            (
                "LIBRARY_DIR".into(),
                root.join("libraries").to_string_lossy().into_owned(),
            ),
            ("INSTALLER".into(), installer.to_string_lossy().into_owned()),
            (
                "MINECRAFT_JAR".into(),
                root.join("versions")
                    .join(&base.id)
                    .join(format!("{}.jar", base.id))
                    .to_string_lossy()
                    .into_owned(),
            ),
            ("MINECRAFT_VERSION".into(), base.id.clone()),
        ]);
        if let Some(data) = profile["data"].as_object() {
            for (key, sides) in data {
                let raw = sides["client"].as_str().ok_or("安装器缺少客户端数据项。")?;
                let value = if raw.starts_with('[') && raw.ends_with(']') {
                    let path = safe_join(
                        &root.join("libraries"),
                        &maven_path(&raw[1..raw.len() - 1])?,
                    )?;
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    tracked.insert(path.clone());
                    path.to_string_lossy().into_owned()
                } else if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
                    raw[1..raw.len() - 1].into()
                } else {
                    let relative = raw.trim_start_matches('/');
                    let path = safe_join(&work.0, relative)?;
                    extract(&mut archive, relative, &path)?;
                    path.to_string_lossy().into_owned()
                };
                values.insert(key.clone(), value);
            }
        }
        let references = regex::Regex::new(r"\{([A-Za-z0-9_]+)\}").unwrap();
        let mut required = BTreeSet::new();
        for processor in &processors {
            for argument in &processor.args {
                for reference in references.captures_iter(argument) {
                    if profile["data"][&reference[1]]["client"]
                        .as_str()
                        .is_some_and(|raw| raw.starts_with('[') && raw.ends_with(']'))
                        && let Some(value) = values.get(&reference[1])
                        && Path::new(value).starts_with(root.join("libraries"))
                    {
                        library_output(root, value)?;
                        required.insert(PathBuf::from(value));
                    }
                }
            }
        }
        let log_path = root.join(".nscl/installers").join(format!("{id}.log"));
        // Keep complete processor output in one log without blocking on full OS pipes.
        File::create(&log_path).map_err(|e| e.to_string())?;
        for (index, processor) in processors.iter().enumerate() {
            downloader.check_cancelled()?;
            downloader.phase(
                &format!(
                    "处理 {} 补丁 {}/{}",
                    selection.kind.label(),
                    index + 1,
                    processors.len()
                ),
                processors.len(),
            );
            let outputs = processor
                .outputs
                .iter()
                .map(|(file, hash)| {
                    Ok((expand(file, &values, root)?, expand(hash, &values, root)?))
                })
                .collect::<Result<Vec<_>>>()?;
            for (file, hash) in &outputs {
                library_output(root, file)?;
                validate_hash(hash)?;
                tracked.insert(PathBuf::from(file));
            }
            if !outputs.is_empty()
                && outputs
                    .iter()
                    .all(|(file, hash)| verified(Path::new(file), hash, None).unwrap_or(false))
            {
                continue;
            }
            let arguments = processor
                .args
                .iter()
                .map(|arg| expand(arg, &values, root))
                .collect::<Result<Vec<_>>>()?;
            if arguments
                .iter()
                .any(|argument| argument == "DOWNLOAD_MOJMAPS")
            {
                let output = arguments
                    .windows(2)
                    .find(|pair| pair[0] == "--output")
                    .map(|pair| &pair[1])
                    .ok_or("映射下载步骤缺少输出路径。")?;
                library_output(root, output)?;
                let artifact = base
                    .downloads
                    .get("client_mappings")
                    .ok_or("该 Minecraft 版本缺少官方客户端映射。")?;
                downloader.download(&artifact_job(artifact, PathBuf::from(output)))?;
                tracked.insert(PathBuf::from(output));
            } else {
                let jar = safe_join(&root.join("libraries"), &maven_path(&processor.jar)?)?;
                let mut classpath = vec![jar.clone()];
                for library in &processor.classpath {
                    classpath.push(safe_join(&root.join("libraries"), &maven_path(library)?)?);
                }
                if classpath.iter().any(|file| !file.is_file()) {
                    return Err("补丁处理器依赖文件缺失。".into());
                }
                let classpath = std::env::join_paths(&classpath).map_err(|e| e.to_string())?;
                let main = main_class(&jar)?;
                let log = OpenOptions::new()
                    .append(true)
                    .open(&log_path)
                    .map_err(|e| e.to_string())?;
                let stderr = log.try_clone().map_err(|e| e.to_string())?;
                let mut command = hidden_command(&java);
                command
                    .args([
                        "-Djava.awt.headless=true",
                        "-Djava.net.useSystemProxies=true",
                        "-Dsun.net.client.defaultConnectTimeout=15000",
                        "-Dsun.net.client.defaultReadTimeout=30000",
                    ])
                    .arg(format!("-Djava.io.tmpdir={}", work.0.display()))
                    .arg("-cp")
                    .arg(classpath)
                    .arg(main)
                    .args(arguments)
                    .current_dir(root)
                    .stdin(Stdio::null())
                    .stdout(Stdio::from(log))
                    .stderr(Stdio::from(stderr));
                let mut child = command
                    .spawn()
                    .map_err(|e| format!("无法执行补丁处理器：{e}"))?;
                let deadline = Instant::now() + Duration::from_secs(20 * 60);
                let status = loop {
                    if let Err(error) = downloader.check_cancelled() {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(error);
                    }
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(format!("补丁处理超时，请查看 {}", log_path.display()));
                    }
                    match child.try_wait() {
                        Ok(Some(status)) => break status,
                        Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                        Err(error) => {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err(error.to_string());
                        }
                    }
                };
                if !status.success() {
                    return Err(format!(
                        "{} 补丁步骤 {} 失败（退出码 {:?}），请查看 {}",
                        selection.kind.label(),
                        index + 1,
                        status.code(),
                        log_path.display()
                    ));
                }
            }
            for (file, hash) in outputs {
                if !verified(Path::new(&file), &hash, None)? {
                    return Err(format!("补丁输出校验失败：{file}"));
                }
            }
        }
        for (key, expected) in &values {
            if let Some(key) = key.strip_suffix("_SHA")
                && let Some(file) = values.get(key)
                && !verified(Path::new(file), expected, None)?
            {
                return Err(format!("加载器生成文件校验失败：{key}"));
            }
        }
        for file in required {
            if !file.is_file() {
                return Err(format!(
                    "补丁处理未生成必要文件：{}。请查看 {}",
                    file.display(),
                    log_path.display()
                ));
            }
        }
    }
    let files = tracked
        .into_iter()
        .filter(|path| path.is_file())
        .map(|path| record(root, &path))
        .collect::<Result<_>>()?;
    Ok((child, files))
}

fn entry(archive: &mut ZipArchive<File>, name: &str, limit: u64) -> Result<Vec<u8>> {
    let file = archive
        .by_name(name)
        .map_err(|_| format!("加载器安装包缺少 {name}。"))?;
    if file.size() > limit {
        return Err("安装包条目过大。".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("安装包条目解压后过大。".into());
    }
    Ok(bytes)
}

fn extract(archive: &mut ZipArchive<File>, name: &str, destination: &Path) -> Result<()> {
    let bytes = entry(archive, name, 256 * 1024 * 1024)?;
    fs::create_dir_all(destination.parent().ok_or("无效解压路径。")?).map_err(|e| e.to_string())?;
    fs::write(destination, bytes).map_err(|e| e.to_string())
}

pub(super) fn expand(raw: &str, values: &BTreeMap<String, String>, root: &Path) -> Result<String> {
    let pattern = regex::Regex::new(r"\{([A-Za-z0-9_]+)\}").unwrap();
    let mut missing = None;
    // Values are already resolved paths or quoted literals. Substitute once so braces
    // inside an actual user directory are not interpreted as new installer variables.
    let expanded = pattern
        .replace_all(raw, |capture: &regex::Captures<'_>| {
            match values.get(&capture[1]) {
                Some(value) => value.clone(),
                None => {
                    missing = Some(capture[1].to_owned());
                    String::new()
                }
            }
        })
        .into_owned();
    if let Some(key) = missing {
        return Err(format!("不支持的安装器变量：{key}"));
    }
    if expanded.starts_with('[') && expanded.ends_with(']') {
        return Ok(safe_join(
            &root.join("libraries"),
            &maven_path(&expanded[1..expanded.len() - 1])?,
        )?
        .to_string_lossy()
        .into_owned());
    }
    Ok(expanded.trim_matches('\'').to_owned())
}

fn library_output(root: &Path, value: &str) -> Result<()> {
    let path = Path::new(value);
    let relative = path
        .strip_prefix(root.join("libraries"))
        .map_err(|_| "补丁输出必须位于 libraries 目录内。")?;
    safe_join(
        Path::new(""),
        &relative.to_string_lossy().replace('\\', "/"),
    )?;
    Ok(())
}

fn record(root: &Path, path: &Path) -> Result<Artifact> {
    let relative = path
        .strip_prefix(root.join("libraries"))
        .map_err(|_| "生成文件路径越界。")?
        .to_string_lossy()
        .replace('\\', "/");
    safe_join(Path::new(""), &relative)?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let mut hash = Sha1::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(Artifact {
        url: String::new(),
        path: Some(relative),
        sha1: format!("{:x}", hash.finalize()),
        size: Some(size),
    })
}

fn main_class(jar: &Path) -> Result<String> {
    let mut archive =
        ZipArchive::new(File::open(jar).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let bytes = entry(&mut archive, "META-INF/MANIFEST.MF", 1024 * 1024)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| "处理器 manifest 编码无效。")?
        .replace("\r\n ", "")
        .replace("\n ", "");
    let main = text
        .lines()
        .find_map(|line| line.strip_prefix("Main-Class:"))
        .ok_or("处理器缺少 Main-Class。")?
        .trim()
        .to_owned();
    if main.is_empty()
        || !main
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._$".contains(&c))
    {
        return Err("处理器 Main-Class 无效。".into());
    }
    Ok(main)
}

struct WorkDirectory(PathBuf);
impl WorkDirectory {
    fn new(root: &Path) -> Result<Self> {
        let directory = root
            .join(".nscl/installer-work")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        Ok(Self(directory))
    }
}
impl Drop for WorkDirectory {
    fn drop(&mut self) {
        // This UUID directory contains only temporary extracted patch data.
        if let (Ok(directory), Some(parent)) = (fs::canonicalize(&self.0), self.0.parent())
            && let Ok(parent) = fs::canonicalize(parent)
            && directory.parent() == Some(parent.as_path())
        {
            let _ = fs::remove_dir_all(directory);
        }
    }
}
