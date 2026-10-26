use super::{ContentKind, ContentVersion, files};
use crate::minecraft::{self, Downloader, Result};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Save a verified JAR without touching an instance or its installation records.
pub fn download_mod(
    directory: &Path,
    version: &ContentVersion,
    downloader: &Downloader,
) -> Result<PathBuf> {
    minecraft::validate_id(&version.project_id)?;
    minecraft::validate_id(&version.id)?;
    let file = version.file(ContentKind::Mod)?;
    let relative = format!("{}/{}/{}", version.project_id, version.id, file.filename);
    let destination = files::safe_path(directory, &relative)?;
    downloader.check_cancelled()?;
    if files::verified(&destination, &file.hashes, file.size)? {
        return Ok(destination);
    }
    if destination.exists() {
        return Err("下载目录存在同名但内容不同的文件，请移走该文件后重试。".into());
    }
    downloader.phase(&format!("下载模组：{}", version.name), 0);
    let cached = files::fetch(
        directory,
        std::slice::from_ref(&file.url),
        &file.hashes,
        file.size,
        &file.filename,
        downloader,
    )?;
    let mut stage = files::Stage::new(directory)?;
    let staged = stage.path.join("download.jar");
    fs::copy(cached, &staged).map_err(|e| e.to_string())?;
    stage.commit(
        directory,
        BTreeMap::from([(relative, Some(staged))]),
        downloader,
    )?;
    Ok(destination)
}
