use super::{
    ContentKind, ContentVersion, InstalledFile, Modrinth, PackRecord, REGISTRY, Registry, files,
};
use crate::{
    instances::Instance,
    minecraft::{self, Downloader, LoaderKind, LoaderVersion, Result},
    store::Settings,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::ZipArchive;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackIndex {
    format_version: u32,
    game: String,
    version_id: String,
    name: String,
    files: Vec<PackFile>,
    dependencies: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackFile {
    path: String,
    hashes: BTreeMap<String, String>,
    file_size: u64,
    downloads: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}
impl PackFile {
    fn active(&self, optional: bool) -> bool {
        match self
            .env
            .get("client")
            .map(String::as_str)
            .unwrap_or("required")
        {
            "required" => true,
            "optional" => optional,
            _ => false,
        }
    }
}

#[derive(Clone)]
pub struct PackPlan {
    pub name: String,
    pub version: String,
    pub minecraft: String,
    pub loader: Option<LoaderVersion>,
    pub include_optional: bool,
    pub optional_files: usize,
    archive: PathBuf,
    archive_hash: InstalledFile,
    index: PackIndex,
    overrides: BTreeMap<String, String>,
}
impl PackPlan {
    pub fn download_count(&self) -> usize {
        self.index
            .files
            .iter()
            .filter(|f| f.active(self.include_optional))
            .count()
    }
    pub fn override_count(&self) -> usize {
        self.overrides.len()
    }
    pub fn label(&self) -> String {
        format!(
            "{} {} · Minecraft {} · {}",
            self.name,
            self.version,
            self.minecraft,
            self.loader
                .as_ref()
                .map(|l| format!("{} {}", l.kind.label(), l.version))
                .unwrap_or_else(|| "原版".into())
        )
    }
}

fn pack_path(relative: &str) -> Result<()> {
    minecraft::safe_join(Path::new(""), relative)?;
    let first = relative
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        first.as_str(),
        "nscl-instance.json" | ".nscl-content.json" | ".nscl" | ".nscl-content.lock"
    ) {
        return Err("整合包不能覆盖启动器的实例配置或安装记录。".into());
    }
    Ok(())
}

fn archive_entry(archive: &mut ZipArchive<fs::File>, name: &str, limit: u64) -> Result<Vec<u8>> {
    let entry = archive
        .by_name(name)
        .map_err(|_| format!("整合包缺少 {name}。"))?;
    if entry.size() > limit {
        return Err("整合包元数据过大。".into());
    }
    let mut bytes = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("整合包条目解压后过大。".into());
    }
    Ok(bytes)
}

pub fn inspect_pack(root: &Path, path: &Path) -> Result<PackPlan> {
    if !path.is_file() {
        return Err("请选择可读取的 .mrpack 文件。".into());
    }
    let hash = files::record(path, String::new())?;
    let mut archive = ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("无效的 .mrpack 压缩包：{e}"))?;
    if archive.by_name("modrinth.index.json").is_err() {
        return Err(if archive.by_name("manifest.json").is_ok() {
            "这是 CurseForge 格式的整合包，当前只支持 Modrinth .mrpack / ZIP，无法自动安装。"
        } else {
            "未检测到 modrinth.index.json，请拖入有效的 Modrinth 整合包。"
        }
        .into());
    }
    if archive.len() > 20000 {
        return Err("整合包包含过多压缩条目。".into());
    }
    let mut entries = BTreeSet::new();
    let mut expanded_size = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.name().trim_end_matches('/');
        minecraft::safe_join(Path::new(""), name)?;
        if !entries.insert(name.to_ascii_lowercase()) {
            return Err("整合包包含重复或仅大小写不同的压缩条目。".into());
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("整合包不能包含符号链接。".into());
        }
        if entry.size() > files::MAX_FILE {
            return Err("整合包单个条目超过 512 MiB。".into());
        }
        expanded_size = expanded_size
            .checked_add(entry.size())
            .ok_or("整合包体积溢出。")?;
        if expanded_size > 4 * 1024 * 1024 * 1024 {
            return Err("整合包解压大小超过 4 GiB。".into());
        }
    }
    let index: PackIndex = serde_json::from_slice(&archive_entry(
        &mut archive,
        "modrinth.index.json",
        8 * 1024 * 1024,
    )?)
    .map_err(|e| format!("整合包清单无效：{e}"))?;
    if index.format_version != 1 || index.game != "minecraft" {
        return Err("只支持 Minecraft 的 Modrinth v1 整合包。".into());
    }
    if index.name.trim().is_empty()
        || index.name.chars().any(char::is_control)
        || index.name.len() > 512
        || index.version_id.trim().is_empty()
        || index.version_id.chars().any(char::is_control)
        || index.version_id.len() > 256
    {
        return Err("整合包名称或版本标识无效。".into());
    }
    if index.files.len() > 10000 {
        return Err("整合包下载文件超过 10000 个。".into());
    }
    let minecraft = index
        .dependencies
        .get("minecraft")
        .ok_or("整合包没有声明 Minecraft 版本。")?
        .clone();
    minecraft::validate_id(&minecraft)?;
    let mut loader = None;
    for (key, version) in &index.dependencies {
        let kind = match key.as_str() {
            "minecraft" => continue,
            "forge" => LoaderKind::Forge,
            "fabric-loader" => LoaderKind::Fabric,
            "neoforge" => LoaderKind::NeoForge,
            _ => {
                return Err(format!(
                    "整合包要求尚不支持的组件：{key}。目前支持原版、Forge、Fabric 和 NeoForge。"
                ));
            }
        };
        if loader.is_some() {
            return Err("整合包同时声明了多个加载器，无法安全安装。".into());
        }
        let selection = LoaderVersion {
            kind,
            minecraft: minecraft.clone(),
            version: version.clone(),
            stable: true,
        };
        selection.id()?;
        loader = Some(selection);
    }
    let mut destinations = BTreeMap::new();
    let mut download_size = expanded_size;
    for file in &index.files {
        pack_path(&file.path)?;
        if destinations
            .insert(file.path.to_ascii_lowercase(), file.path.clone())
            .is_some()
        {
            return Err("整合包下载清单有重复文件路径。".into());
        }
        files::validate_hashes(&file.hashes)?;
        if file.file_size > files::MAX_FILE
            || file.downloads.is_empty()
            || file.downloads.len() > 16
        {
            return Err("整合包资源大小或下载地址数量无效。".into());
        }
        for url in &file.downloads {
            files::download_url(url)?;
        }
        for side in ["client", "server"] {
            if file
                .env
                .get(side)
                .is_some_and(|env| !matches!(env.as_str(), "required" | "optional" | "unsupported"))
            {
                return Err("整合包含有未知的客户端/服务端文件规则。".into());
            }
        }
        download_size = download_size
            .checked_add(file.file_size)
            .ok_or("整合包体积溢出。")?;
        if download_size > 4 * 1024 * 1024 * 1024 {
            return Err("整合包安装内容超过 4 GiB。".into());
        }
    }
    let mut overrides = BTreeMap::new();
    // Client-specific overrides take precedence over common overrides.
    for prefix in ["overrides/", "client-overrides/"] {
        for i in 0..archive.len() {
            let entry = archive.by_index(i).map_err(|e| e.to_string())?;
            if entry.is_dir() {
                continue;
            }
            if let Some(relative) = entry.name().strip_prefix(prefix) {
                pack_path(relative)?;
                let key = relative.to_ascii_lowercase();
                if destinations
                    .get(&key)
                    .is_some_and(|existing| existing != relative)
                {
                    return Err("整合包文件路径存在大小写冲突。".into());
                }
                destinations.insert(key, relative.to_owned());
                overrides.insert(relative.to_owned(), entry.name().to_owned());
            }
        }
    }
    let cache = files::safe_path(
        root,
        &format!(".nscl/content-cache/{}/pack.mrpack", hash.hashes["sha512"]),
    )?;
    if !files::verified(&cache, &hash.hashes, hash.size)? {
        fs::create_dir_all(cache.parent().unwrap()).map_err(|e| e.to_string())?;
        if cache != path {
            fs::copy(path, &cache).map_err(|e| e.to_string())?;
        }
        if !files::verified(&cache, &hash.hashes, hash.size)? {
            return Err("整合包在检查期间发生变化，请重新导入。".into());
        }
    }
    let optional_files = index
        .files
        .iter()
        .filter(|f| f.env.get("client").is_some_and(|v| v == "optional"))
        .count();
    Ok(PackPlan {
        name: index.name.clone(),
        version: index.version_id.clone(),
        minecraft,
        loader,
        include_optional: false,
        optional_files,
        archive: cache,
        archive_hash: hash,
        index,
        overrides,
    })
}

pub fn plan_pack(root: &Path, version: &ContentVersion, api: &Modrinth) -> Result<PackPlan> {
    api.check_project(&version.project_id, ContentKind::Modpack)?;
    let file = version.file(ContentKind::Modpack)?;
    api.downloader.phase("下载并检查整合包清单", 1);
    let archive = files::fetch(
        root,
        std::slice::from_ref(&file.url),
        &file.hashes,
        file.size,
        &file.filename,
        &api.downloader,
    )?;
    let plan = inspect_pack(root, &archive)?;
    if !version.game_versions.contains(&plan.minecraft)
        || plan
            .loader
            .as_ref()
            .is_some_and(|loader| !version.loaders.iter().any(|l| l == loader.kind.key()))
    {
        return Err("整合包清单的游戏版本或加载器与发布信息不一致。".into());
    }
    Ok(plan)
}

pub fn install_pack(
    root: &Path,
    plan: &PackPlan,
    settings: &Settings,
    downloader: &Downloader,
) -> Result<Instance> {
    downloader.check_cancelled()?;
    if !files::verified(
        &plan.archive,
        &plan.archive_hash.hashes,
        plan.archive_hash.size,
    )? {
        return Err("整合包缓存损坏，请重新检查安装方案。".into());
    }
    let stage = files::Stage::new(root)?;
    let payload = stage.directory("instance")?;
    let mut records = BTreeMap::<String, InstalledFile>::new();
    let active: Vec<_> = plan
        .index
        .files
        .iter()
        .filter(|f| f.active(plan.include_optional))
        .collect();
    let key = |file: &PackFile| {
        (
            file.hashes["sha512"].to_ascii_lowercase(),
            file.path
                .rsplit('/')
                .next()
                .unwrap_or("resource")
                .to_owned(),
        )
    };
    let mut unique = BTreeMap::<_, &PackFile>::new();
    for file in &active {
        if plan.loader.is_none() && file.path.starts_with("mods/") && file.path.ends_with(".jar") {
            return Err("整合包包含模组 JAR，但没有声明支持的加载器。".into());
        }
        if let Some(existing) = unique.insert(key(file), file)
            && (existing.hashes != file.hashes || existing.file_size != file.file_size)
        {
            return Err("相同资源缓存键对应不同的校验信息。".into());
        }
    }
    let unique: Vec<_> = unique.into_iter().collect();
    let mut cached = BTreeMap::new();
    downloader.phase("下载并校验整合包资源", unique.len());
    for batch in unique.chunks(4) {
        let results = std::thread::scope(|scope| {
            let jobs: Vec<_> = batch
                .iter()
                .map(|(key, file)| {
                    (
                        key.clone(),
                        scope.spawn(move || {
                            files::fetch(
                                root,
                                &file.downloads,
                                &file.hashes,
                                file.file_size,
                                file.path.rsplit('/').next().unwrap_or("resource"),
                                downloader,
                            )
                        }),
                    )
                })
                .collect();
            jobs.into_iter()
                .map(|(key, job)| {
                    let path = job.join().map_err(|_| "资源下载线程异常。".to_owned())??;
                    downloader.complete("整合包资源已通过校验");
                    Ok((key, path))
                })
                .collect::<Result<Vec<_>>>()
        })?;
        cached.extend(results);
    }
    for file in active {
        downloader.check_cancelled()?;
        let cache = cached.get(&key(file)).ok_or("整合包资源缓存缺失。")?;
        let destination = files::safe_path(&payload, &file.path)?;
        fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(cache, &destination).map_err(|e| e.to_string())?;
        records.insert(
            file.path.clone(),
            InstalledFile {
                path: file.path.clone(),
                hashes: file.hashes.clone(),
                size: file.file_size,
            },
        );
    }
    let mut archive = ZipArchive::new(fs::File::open(&plan.archive).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    downloader.phase("写入整合包客户端配置", plan.overrides.len());
    for (relative, name) in &plan.overrides {
        downloader.check_cancelled()?;
        let destination = files::safe_path(&payload, relative)?;
        fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut entry = archive.by_name(name).map_err(|e| e.to_string())?;
        let mut file = fs::File::create(&destination).map_err(|e| e.to_string())?;
        let mut buffer = [0; 65536];
        let mut written = 0;
        loop {
            downloader.check_cancelled()?;
            let read = entry.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            written += read as u64;
            if written > files::MAX_FILE {
                return Err("整合包覆盖文件过大。".into());
            }
            file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        }
        drop(file);
        if plan.loader.is_none() && relative.starts_with("mods/") && relative.ends_with(".jar") {
            return Err("整合包含模组 JAR，但没有声明加载器。".into());
        }
        records.insert(
            relative.clone(),
            files::record(&destination, relative.clone())?,
        );
    }
    downloader.phase("准备整合包要求的 Minecraft 与加载器", 0);
    let manifest = minecraft::manifest(downloader)?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == plan.minecraft)
        .ok_or("官方版本列表中没有整合包要求的 Minecraft 版本。")?;
    let installed = match &plan.loader {
        Some(loader) => {
            minecraft::install_loader(root, entry, loader, settings.configured_java(), downloader)?
        }
        None => minecraft::install(root, entry, downloader)?,
    };
    let instance = Instance {
        id: format!("pack-{}", uuid::Uuid::new_v4().simple()),
        name: format!("{} {}", plan.name, plan.version)
            .chars()
            .take(64)
            .collect(),
        version_id: installed.id,
        ..Default::default()
    };
    instance.validate()?;
    let record = Registry {
        mods: BTreeMap::new(),
        pack: Some(PackRecord {
            name: plan.name.clone(),
            version: plan.version.clone(),
            minecraft: plan.minecraft.clone(),
            loader: plan.loader.clone(),
            files: records.into_values().collect(),
        }),
    };
    crate::store::write_json(&payload.join(REGISTRY), &record)?;
    crate::store::write_json(&payload.join("nscl-instance.json"), &instance)?;
    downloader.check_cancelled()?;
    let destination = files::safe_path(root, &format!("instances/{}", instance.id))?;
    if destination.exists() {
        return Err("整合包实例目录已存在，请重试。".into());
    }
    fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::rename(&payload, &destination).map_err(|e| format!("发布整合包实例失败：{e}"))?;
    Ok(instance)
}
