use super::{
    Result,
    download::{safe_join, verified},
    external,
    install::{self, Platform},
    model::{AssetIndex, Version},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
pub(crate) fn runtime_files(root: &Path, id: &str) -> Result<Vec<PathBuf>> {
    let marker = root.join("versions").join(id).join("nscl-instance.json");
    let (version, client, extra) = if marker.exists() {
        let installed: install::InstalledVersion = install::read_json(&marker)?;
        let metadata = root.join("versions").join(id).join(format!("{id}.json"));
        if installed.id != id
            || installed.platform != Platform::current().key()
            || !verified(&metadata, &installed.metadata_sha1, None)?
        {
            return Err("来源版本元数据损坏，请先修复。".into());
        }
        let version: Version = install::read_json(&metadata)?;
        (
            version,
            root.join("versions").join(id).join(format!("{id}.jar")),
            installed.extra_files,
        )
    } else {
        let (version, client) = external::prepare(root, id, &Platform::current())?;
        (version, client, Vec::new())
    };
    let index_path = safe_join(
        root,
        &format!("assets/indexes/{}.json", version.asset_index.id),
    )?;
    if !verified(
        &index_path,
        &version.asset_index.artifact.sha1,
        version.asset_index.artifact.size,
    )? {
        return Err("来源资源索引损坏。".into());
    }
    let index: AssetIndex = install::read_json(&index_path)?;
    let plan = install::plan(root, &version, &index, &Platform::current())?;
    let mut files: BTreeSet<_> = external::runtime_profiles(root, id)?.into_iter().collect();
    files.insert(client.clone());
    files.insert(index_path);
    let logging_paths = match &version.logging {
        Some(logging) => super::logging::paths(root, id, &client, None, &logging.client.file.id)?,
        None => Vec::new(),
    };
    let client_path = root.join("versions").join(id).join(format!("{id}.jar"));
    for mut job in plan.jobs {
        if job.path == client_path {
            job.path = client.clone();
        }
        if logging_paths.contains(&job.path)
            && !job.path.exists()
            && let Some(path) = logging_paths
                .iter()
                .find(|p| verified(p, &job.sha1, job.size).unwrap_or(false))
        {
            files.insert(path.clone());
            continue;
        }
        if !verified(&job.path, &job.sha1, job.size)? {
            return Err(format!("来源运行文件缺失或损坏：{}", job.path.display()));
        }
        files.insert(job.path);
    }
    for artifact in extra {
        let path = safe_join(
            root,
            artifact.path.as_deref().ok_or("加载器额外文件缺少路径。")?,
        )?;
        if !verified(&path, &artifact.sha1, artifact.size)? {
            return Err("来源加载器生成文件缺失或损坏，请先修复。".into());
        }
        files.insert(path);
    }
    Ok(files.into_iter().collect())
}
