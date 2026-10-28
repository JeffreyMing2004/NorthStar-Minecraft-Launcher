use super::files;
use crate::{
    instances::Instance,
    minecraft::{self, Downloader, LoaderKind, Result},
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct PackExportOptions {
    pub name: String,
    pub version: String,
    pub summary: String,
    pub include_saves: bool,
    pub include_settings: bool,
}

impl PackExportOptions {
    pub fn for_instance(instance: &Instance) -> Self {
        Self {
            name: instance.name.clone(),
            version: "1.0.0".into(),
            summary: String::new(),
            include_saves: false,
            include_settings: false,
        }
    }

    pub fn validate(&self) -> Result<()> {
        for (value, limit, label) in [(&self.name, 512, "名称"), (&self.version, 256, "版本")] {
            if value.trim().is_empty() || value.len() > limit || value.chars().any(char::is_control)
            {
                return Err(format!("整合包{label}不能为空、过长或包含控制字符。"));
            }
        }
        if self.summary.len() > 4096 || self.summary.chars().any(char::is_control) {
            return Err("整合包简介过长或包含控制字符。".into());
        }
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportIndex<'a> {
    format_version: u32,
    game: &'static str,
    version_id: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    summary: &'a str,
    files: [(); 0],
    dependencies: std::collections::BTreeMap<&'static str, String>,
}

struct OutputGuard(Option<PathBuf>);
impl Drop for OutputGuard {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

fn collect(
    directory: &Path,
    relative: &str,
    result: &mut Vec<(String, u64)>,
    seen: &mut BTreeSet<String>,
    total: &mut u64,
    depth: usize,
    progress: &Downloader,
) -> Result<()> {
    progress.check_cancelled()?;
    if depth > 64 {
        return Err("导出目录层级超过 64 层。".into());
    }
    let path = files::safe_path(directory, relative)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("无法读取 {relative}：{e}")),
    };
    if !seen.insert(relative.to_ascii_lowercase()) {
        return Err(format!("导出文件路径存在大小写冲突：{relative}"));
    }
    if seen.len() > 20000 {
        return Err("导出文件和目录超过 20000 个。".into());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| format!("无法读取 {relative}：{e}"))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "导出文件名必须为 UTF-8。")?;
            collect(
                directory,
                &format!("{relative}/{name}"),
                result,
                seen,
                total,
                depth + 1,
                progress,
            )?;
        }
    } else if metadata.is_file() {
        if metadata.len() > files::MAX_FILE {
            return Err(format!("导出单个文件不能超过 512 MiB：{relative}"));
        }
        *total += metadata.len();
        // Leave room for the index within the importer's 4 GiB expanded-size limit.
        if *total > 4 * 1024 * 1024 * 1024 - 8 * 1024 * 1024 {
            return Err("导出内容超过整合包 4 GiB 限制。".into());
        }
        result.push((relative.to_owned(), metadata.len()));
    } else {
        return Err(format!("不能导出特殊文件：{relative}"));
    }
    Ok(())
}

/// Export only game content; launcher metadata, accounts, caches and runtime files stay local.
/// All payload is stored as client overrides so exporting never needs a network lookup.
pub fn export_pack(
    root: &Path,
    instance: &Instance,
    options: &PackExportOptions,
    destination: &Path,
    progress: &Downloader,
) -> Result<PathBuf> {
    progress.check_cancelled()?;
    options.validate()?;
    let installed = minecraft::installed_versions(root)
        .into_iter()
        .find(|v| v.id == instance.version_id)
        .ok_or("实例的游戏或加载器尚未完成安装。")?;
    let mut target = super::Target::from_installed(instance, &installed)?;
    if installed.external && target.loader.is_none() {
        target.minecraft = minecraft::external::base_game_version(root, &instance.version_id)?;
    }
    minecraft::validate_id(&target.minecraft)?;
    let mut dependencies =
        std::collections::BTreeMap::from([("minecraft", target.minecraft.clone())]);
    if let Some(loader) = target.loader {
        let version = target
            .loader_version
            .clone()
            .ok_or("实例缺少加载器版本。")?;
        minecraft::LoaderVersion {
            kind: loader,
            minecraft: target.minecraft.clone(),
            version: version.clone(),
            stable: true,
        }
        .id()?;
        dependencies.insert(
            match loader {
                LoaderKind::Fabric => "fabric-loader",
                LoaderKind::Forge => "forge",
                LoaderKind::NeoForge => "neoforge",
            },
            version,
        );
    }
    if !destination
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mrpack"))
    {
        return Err("导出文件必须使用 .mrpack 扩展名。".into());
    }
    let result_path = std::path::absolute(destination).map_err(|e| e.to_string())?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|e| format!("请选择已存在的导出目录：{e}"))?;
    let destination = parent.join(destination.file_name().ok_or("请选择导出文件名。")?);
    if destination.exists() {
        return Err("导出文件已存在，请选择新的文件名。".into());
    }
    let directory = instance.game_directory(root)?;
    files::reject_link(&directory)?;
    let mut folders = vec![
        "mods",
        "config",
        "defaultconfigs",
        "kubejs",
        "scripts",
        "resourcepacks",
        "shaderpacks",
        "resources",
        "patchouli_books",
        "global_packs",
        "icon.png",
        "pack.png",
    ];
    if options.include_saves {
        folders.push("saves");
    }
    if options.include_settings {
        folders.extend([
            "options.txt",
            "optionsof.txt",
            "optionsshaders.txt",
            "servers.dat",
        ]);
    }
    progress.phase("扫描整合包导出内容", 0);
    let (mut payload, mut seen, mut total) = (Vec::new(), BTreeSet::new(), 0);
    for relative in folders {
        // Never let an export contain itself, including when the output does not exist yet.
        let selected = files::safe_path(&directory, relative)?;
        if selected.exists()
            && destination.starts_with(fs::canonicalize(&selected).map_err(|e| e.to_string())?)
        {
            return Err("请将整合包保存到导出内容目录之外。".into());
        }
        collect(
            &directory,
            relative,
            &mut payload,
            &mut seen,
            &mut total,
            0,
            progress,
        )?;
    }
    payload.sort_by(|a, b| a.0.cmp(&b.0));
    if target.loader.is_none()
        && payload
            .iter()
            .any(|(p, _)| p.starts_with("mods/") && p.ends_with(".jar"))
    {
        return Err("实例包含模组 JAR，但没有支持的加载器，无法导出可安装的整合包。".into());
    }
    let index = serde_json::to_vec_pretty(&ExportIndex {
        format_version: 1,
        game: "minecraft",
        version_id: options.version.trim(),
        name: options.name.trim(),
        summary: options.summary.trim(),
        files: [],
        dependencies,
    })
    .map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(
        ".nscl-export-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let _temporary_guard = OutputGuard(Some(temporary));
    let mut archive = zip::ZipWriter::new(file);
    let zip_options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    archive
        .start_file("modrinth.index.json", zip_options)
        .map_err(|e| e.to_string())?;
    archive.write_all(&index).map_err(|e| e.to_string())?;
    progress.phase("压缩整合包文件", payload.len());
    let mut buffer = [0; 65536];
    for (relative, size) in &payload {
        progress.check_cancelled()?;
        let source = files::safe_path(&directory, relative)?;
        let mut source = fs::File::open(source).map_err(|e| format!("无法读取 {relative}：{e}"))?;
        archive
            .start_file(format!("client-overrides/{relative}"), zip_options)
            .map_err(|e| e.to_string())?;
        let mut written = 0u64;
        loop {
            progress.check_cancelled()?;
            let read = source.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            written += read as u64;
            if written > *size {
                return Err(format!("导出时文件发生变化，请重试：{relative}"));
            }
            archive
                .write_all(&buffer[..read])
                .map_err(|e| e.to_string())?;
        }
        if written != *size {
            return Err(format!("导出时文件发生变化，请重试：{relative}"));
        }
        progress.complete(relative);
    }
    let mut archive = archive.finish().map_err(|e| e.to_string())?;
    archive.sync_all().map_err(|e| e.to_string())?;
    if archive.metadata().map_err(|e| e.to_string())?.len() > files::MAX_FILE {
        return Err("导出压缩包超过 512 MiB，无法重新导入；请减少导出内容。".into());
    }
    progress.check_cancelled()?;
    progress.phase("保存整合包", 1);
    // create_new prevents accidental replacement even if another process creates the path.
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|e| format!("无法创建导出文件（不会覆盖已有文件）：{e}"))?;
    let mut output_guard = OutputGuard(Some(destination.clone()));
    // Declared after the guard so Windows closes the file before cleanup on any error.
    let mut output = file;
    use std::io::{Seek, SeekFrom};
    archive
        .seek(SeekFrom::Start(0))
        .map_err(|e| e.to_string())?;
    loop {
        progress.check_cancelled()?;
        let read = archive.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|e| e.to_string())?;
    }
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    progress.check_cancelled()?;
    output_guard.0 = None;
    progress.complete("整合包导出完成");
    Ok(result_path)
}

#[cfg(test)]
mod tests;
