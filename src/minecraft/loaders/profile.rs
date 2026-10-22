use super::super::download::{validate_hash, verified};
use super::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn maven_path(coordinate: &str) -> Result<String> {
    let (coordinate, extension) = coordinate.split_once('@').unwrap_or((coordinate, "jar"));
    let parts: Vec<_> = coordinate.split(':').collect();
    if !(3..=4).contains(&parts.len()) {
        return Err("Maven 坐标格式无效。".into());
    }
    for part in parts.iter().copied().chain([extension]) {
        validate_id(part)?;
    }
    let classifier = parts
        .get(3)
        .map_or(String::new(), |value| format!("-{value}"));
    let path = format!(
        "{}/{}/{}/{}-{}{classifier}.{extension}",
        parts[0].replace('.', "/"),
        parts[1],
        parts[2],
        parts[1],
        parts[2]
    );
    safe_join(Path::new(""), &path)?;
    Ok(path)
}

pub(super) fn library_key(name: &str) -> String {
    let (name, extension) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<_> = name.split(':').collect();
    format!(
        "{}:{}:{}@{extension}",
        parts.first().unwrap_or(&""),
        parts.get(1).unwrap_or(&""),
        parts.get(3).unwrap_or(&"")
    )
}

pub(super) fn normalize_libraries(
    root: &Path,
    child: &mut Value,
    kind: LoaderKind,
    downloader: &Downloader,
) -> Result<()> {
    let Some(libraries) = child["libraries"].as_array_mut() else {
        return Err("加载器缺少库文件清单。".into());
    };
    for library in libraries {
        downloader.check_cancelled()?;
        let name = library["name"].as_str().ok_or("库文件缺少 Maven 坐标。")?;
        let relative = maven_path(name)?;
        let path = safe_join(&root.join("libraries"), &relative)?;
        if library["downloads"]["artifact"].is_object() {
            let mut artifact: Artifact =
                serde_json::from_value(library["downloads"]["artifact"].clone())
                    .map_err(|e| e.to_string())?;
            if artifact.path.as_deref() != Some(relative.as_str()) {
                return Err("加载器的 Maven 坐标与库文件路径不一致。".into());
            }
            if artifact.url.is_empty() && !verified(&path, &artifact.sha1, artifact.size)? {
                return Err(format!("安装包内嵌库缺失或损坏：{name}"));
            }
            artifact.sha1.make_ascii_lowercase();
            validate_hash(&artifact.sha1)?;
            continue;
        }
        let default_repo = if kind == LoaderKind::Fabric {
            "https://maven.fabricmc.net/"
        } else {
            "https://maven.minecraftforge.net/"
        };
        let mut repository = library["url"]
            .as_str()
            .filter(|url| !url.is_empty())
            .unwrap_or(default_repo)
            .to_owned();
        // Fabric mirror metadata may rewrite the repository root; retain its official
        // fallback rather than confusing a Fabric dependency with a Mojang library.
        if let Some(base) = downloader.source.base()?
            && super::super::source::valid_url(&repository)?.origin() == base.origin()
        {
            repository = default_repo.into();
        }
        let url = format!("{}/{relative}", repository.trim_end_matches('/'));
        let hash = match library["sha1"].as_str() {
            Some(hash) => hash.to_owned(),
            None => downloader.checksum(&format!("{url}.sha1"))?,
        };
        validate_hash(&hash)?;
        library["downloads"] = json!({"artifact":{"path":relative,"url":url,"sha1":hash,"size":library["size"].as_u64()}});
    }
    Ok(())
}

pub(super) fn merge(base: &Version, child: Value, id: &str) -> Result<Version> {
    if child["inheritsFrom"].as_str() != Some(&base.id) {
        return Err("加载器父版本不匹配。".into());
    }
    let mut merged = serde_json::to_value(base).map_err(|e| e.to_string())?;
    merged["id"] = id.into();
    merged["inheritsFrom"] = Value::Null;
    for field in ["mainClass", "type", "javaVersion"] {
        if !child[field].is_null() {
            merged[field] = child[field].clone();
        }
    }
    if let Some(arguments) = child["minecraftArguments"].as_str() {
        merged["minecraftArguments"] = arguments.into();
        merged["arguments"] = Value::Null;
    } else if child["arguments"].is_object() {
        for side in ["jvm", "game"] {
            let mut arguments = merged["arguments"][side]
                .as_array()
                .cloned()
                .unwrap_or_default();
            arguments.extend(
                child["arguments"][side]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
            if !merged["arguments"].is_object() {
                merged["arguments"] = json!({});
            }
            merged["arguments"][side] = Value::Array(arguments);
        }
    }
    let mut seen = BTreeSet::new();
    let libraries = child["libraries"]
        .as_array()
        .ok_or("加载器缺少库清单。")?
        .iter()
        .chain(merged["libraries"].as_array().ok_or("原版缺少库清单。")?)
        .filter(|library| {
            let key = library_key(library["name"].as_str().unwrap_or(""));
            seen.insert(key)
        })
        .cloned()
        .collect();
    merged["libraries"] = Value::Array(libraries);
    let version: Version =
        serde_json::from_value(merged).map_err(|e| format!("加载器启动配置无效：{e}"))?;
    if version.main_class.is_empty()
        || !version
            .main_class
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._$".contains(&c))
    {
        return Err("加载器启动入口无效。".into());
    }
    Ok(version)
}
