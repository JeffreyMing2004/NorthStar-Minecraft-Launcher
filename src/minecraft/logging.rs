use super::{
    Downloader, Result,
    download::{Job, replace_file, safe_join, validate_hash, verified},
    model::{LogFile, Version},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn paths(
    root: &Path,
    version_id: &str,
    client: &Path,
    game_directory: Option<&Path>,
    file_id: &str,
) -> Result<Vec<PathBuf>> {
    let mut paths = vec![safe_join(&root.join("assets/log_configs"), file_id)?];
    let version_directory = super::external::version_directory(root, version_id)?;
    for directory in game_directory
        .into_iter()
        .chain(std::iter::once(version_directory.as_path()))
        .chain(client.parent())
    {
        for path in [
            safe_join(&directory.join("assets/log_configs"), file_id)?,
            safe_join(&directory.join("log_configs"), file_id)?,
            directory.join("downloads/log.json"),
        ] {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

pub(super) fn prepare(
    root: &Path,
    version: &Version,
    client: &Path,
    game_directory: &Path,
    downloader: &Downloader,
) -> Result<Option<PathBuf>> {
    let Some(logging) = &version.logging else {
        return Ok(None);
    };
    let file: &LogFile = &logging.client.file;
    validate_hash(&file.artifact.sha1)?;
    let candidates = paths(root, &version.id, client, Some(game_directory), &file.id)?;
    let destination = &candidates[0];
    for path in &candidates {
        downloader.check_cancelled()?;
        if verified(path, &file.artifact.sha1, file.artifact.size)? {
            if path != destination {
                // PCL's downloads/log.json can contain XML. Preserve the manifest's
                // extension so Log4j selects the matching configuration parser.
                downloader.phase("复用已有启动日志配置", 1);
                fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
                let temporary =
                    destination.with_extension(format!("{}.part", uuid::Uuid::new_v4()));
                let result = (|| {
                    fs::copy(path, &temporary).map_err(|e| e.to_string())?;
                    if !verified(&temporary, &file.artifact.sha1, file.artifact.size)? {
                        return Err("已有日志配置在复制过程中发生变化，请重试。".into());
                    }
                    downloader.check_cancelled()?;
                    replace_file(&temporary, destination)
                })();
                let _ = fs::remove_file(temporary);
                result?;
                downloader.complete("启动日志配置已就绪");
            }
            return Ok(Some(destination.clone()));
        }
    }

    // Other launchers may omit this auxiliary file or leave an empty placeholder.
    // Repair only our shared copy; keep the imported profile and its data intact.
    let path = candidates.into_iter().next().unwrap();
    downloader.phase("补全启动日志配置", 1);
    downloader
        .download(&Job {
            url: file.artifact.url.clone(),
            path: path.clone(),
            sha1: file.artifact.sha1.clone(),
            size: file.artifact.size,
        })
        .map_err(|error| {
            if downloader.check_cancelled().is_err() {
                error
            } else {
                format!(
                    "补全启动日志配置失败：{}\n{error}\n无需重装整个游戏，请检查下载源和网络后重试。",
                    file.id
                )
            }
        })?;
    downloader.complete("启动日志配置已就绪");
    Ok(Some(path))
}
