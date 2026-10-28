//! Read existing launcher profiles without rewriting their metadata or game data.
use super::download::{safe_join, validate_hash};
use super::{
    InstalledVersion, LoaderKind, LoaderVersion, Result, install::Platform, model::Version,
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub fn version_directory(root: &Path, id: &str) -> Result<PathBuf> {
    super::validate_local_id(id)?;
    let parent = root.join("versions");
    crate::instances::reject_link(&parent)?;
    let path = parent.join(id);
    crate::instances::reject_link(&path)?;
    Ok(path)
}

fn read_value(path: &Path) -> Result<Value> {
    crate::instances::reject_link(path)?;
    let file = fs::File::open(path).map_err(|e| format!("读取已有版本配置失败：{e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
        return Err("版本配置过大。".into());
    }
    serde_json::from_reader(file).map_err(|_| "已有版本 JSON 配置无效。".into())
}

fn profile(root: &Path, id: &str, seen: &mut BTreeSet<String>) -> Result<(Value, PathBuf)> {
    if seen.len() >= 16 || !seen.insert(id.into()) {
        return Err("版本继承存在循环或层数过多。".into());
    }
    let dir = version_directory(root, id)?;
    let child = read_value(&dir.join(format!("{id}.json")))?;
    if child["id"].as_str() != Some(id) {
        return Err("版本目录与配置 ID 不匹配。".into());
    }
    let (mut merged, inherited_jar) = if let Some(parent) = child["inheritsFrom"].as_str() {
        profile(root, parent, seen)?
    } else {
        (json!({}), dir.join(format!("{id}.jar")))
    };
    let mut libraries = child["libraries"].as_array().cloned().unwrap_or_default();
    let mut names = BTreeSet::new();
    for library in &libraries {
        let name = library["name"].as_str().ok_or("依赖库缺少名称。")?;
        names.insert(super::loaders::library_key(name));
    }
    for library in merged["libraries"].as_array().into_iter().flatten() {
        let name = library["name"].as_str().ok_or("依赖库缺少名称。")?;
        if !names.contains(&super::loaders::library_key(name)) {
            libraries.push(library.clone());
        }
    }
    let arguments = if child["arguments"].is_object() {
        let mut value = json!({});
        for side in ["jvm", "game"] {
            let mut args = merged["arguments"][side]
                .as_array()
                .cloned()
                .unwrap_or_default();
            args.extend(
                child["arguments"][side]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            value[side] = Value::Array(args);
        }
        Some(value)
    } else {
        None
    };
    for (key, value) in child.as_object().ok_or("版本配置必须是 JSON 对象。")? {
        if !value.is_null() {
            merged[key] = value.clone();
        }
    }
    merged["id"] = id.into();
    merged["inheritsFrom"] = Value::Null;
    merged["libraries"] = Value::Array(libraries);
    if let Some(arguments) = arguments {
        merged["arguments"] = arguments;
    } else if child["minecraftArguments"].is_string() {
        merged["arguments"] = Value::Null;
    }
    let client = if let Some(jar) = child["jar"].as_str() {
        version_directory(root, jar)?.join(format!("{jar}.jar"))
    } else if dir.join(format!("{id}.jar")).is_file() {
        dir.join(format!("{id}.jar"))
    } else {
        inherited_jar
    };
    Ok((merged, client))
}

fn game_version(value: &Value) -> String {
    value["clientVersion"]
        .as_str()
        .or_else(|| {
            value["patches"]
                .as_array()?
                .iter()
                .find(|p| matches!(p["id"].as_str(), Some("game" | "minecraft")))?["version"]
                .as_str()
        })
        .or_else(|| {
            value["libraries"].as_array()?.iter().find_map(|l| {
                l["name"]
                    .as_str()?
                    .strip_prefix("net.fabricmc:intermediary:")
            })
        })
        .unwrap_or_else(|| value["id"].as_str().unwrap_or(""))
        .to_owned()
}
/// Resolve the actual Minecraft version even when an imported profile has a custom name.
pub fn base_game_version(root: &Path, id: &str) -> Result<String> {
    let (value, _) = profile(root, id, &mut BTreeSet::new())?;
    let mut version = game_version(&value);
    // The merged profile uses the child's display ID. Follow inheritance when no
    // explicit game version survived the merge (common for renamed vanilla profiles).
    if version == id {
        let mut current = id.to_owned();
        for _ in 0..16 {
            let directory = version_directory(root, &current)?;
            let source = read_value(&directory.join(format!("{current}.json")))?;
            match source["inheritsFrom"]
                .as_str()
                .or_else(|| source["jar"].as_str())
            {
                Some(parent) => current = parent.to_owned(),
                None => {
                    version = game_version(&source);
                    break;
                }
            }
        }
    }
    super::validate_id(&version)?;
    Ok(version)
}

fn loader(value: &Value) -> Option<LoaderVersion> {
    let libraries = value["libraries"].as_array()?;
    let patch = |id: &str| {
        value["patches"]
            .as_array()?
            .iter()
            .find(|p| p["id"] == id)?["version"]
            .as_str()
    };
    let coordinate = |prefix: &str| {
        libraries
            .iter()
            .find_map(|l| l["name"].as_str()?.strip_prefix(prefix)?.split(':').next())
    };
    let arg = |key: &str| {
        value["arguments"]["game"]
            .as_array()?
            .windows(2)
            .find(|a| a[0] == key)?[1]
            .as_str()
    };
    let (kind, version) =
        if let Some(v) = coordinate("net.fabricmc:fabric-loader:").or_else(|| patch("fabric")) {
            (LoaderKind::Fabric, v)
        } else if let Some(v) = coordinate("net.neoforged:neoforge:")
            .or_else(|| patch("neoforge"))
            .or_else(|| arg("--fml.neoForgeVersion"))
        {
            (LoaderKind::NeoForge, v)
        } else {
            let v = coordinate("net.minecraftforge:forge:")
                .or_else(|| patch("forge"))
                .or_else(|| arg("--fml.forgeVersion"))?;
            (LoaderKind::Forge, v)
        };
    let minecraft = arg("--fml.mcVersion")
        .map(str::to_owned)
        .unwrap_or_else(|| game_version(value));
    let version = version
        .strip_prefix(&format!("{minecraft}-"))
        .unwrap_or(version)
        .to_owned();
    Some(LoaderVersion {
        minecraft,
        kind,
        version,
        stable: true,
    })
}

pub fn describe(root: &Path, id: &str) -> Result<InstalledVersion> {
    let (value, _) = profile(root, id, &mut BTreeSet::new())?;
    let main = value["mainClass"].as_str().ok_or("版本缺少启动入口。")?;
    if main.is_empty()
        || !main
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._$".contains(&b))
    {
        return Err("已有版本启动入口无效。".into());
    }
    if !value["assetIndex"].is_object() || !value["downloads"]["client"].is_object() {
        return Err("已有版本缺少资源索引或客户端清单。".into());
    }
    Ok(InstalledVersion {
        external: true,
        id: id.into(),
        kind: value["type"].as_str().unwrap_or("release").into(),
        java_major: value["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32,
        platform: format!(
            "{}-{}",
            if cfg!(target_os = "macos") {
                "osx"
            } else {
                std::env::consts::OS
            },
            std::env::consts::ARCH
        ),
        metadata_sha1: String::new(),
        loader: loader(&value),
        extra_files: vec![],
    })
}

pub fn initial_isolation(root: &Path, id: &str) -> crate::instances::Isolation {
    use crate::instances::Isolation;
    let dir = root.join("versions").join(id);
    if let Ok(text) = fs::read_to_string(dir.join("PCL/Setup.ini")) {
        for line in text.lines() {
            if let Some(v) = line.trim().strip_prefix("VersionArgumentIndieV2:") {
                return if v.trim().eq_ignore_ascii_case("true") {
                    Isolation::Version
                } else {
                    Isolation::Shared
                };
            }
        }
    }
    if ["mods", "saves", "config", "resourcepacks", "screenshots"]
        .iter()
        .any(|name| dir.join(name).is_dir())
        || dir.join("options.txt").is_file()
    {
        Isolation::Version
    } else {
        Isolation::Shared
    }
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).map_err(|_| format!("已有版本依赖文件缺失：{}", path.display()))?;
    let mut hash = Sha1::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn artifact(value: &mut Value, path: &Path) -> Result<()> {
    if !value["sha1"].as_str().is_some_and(|h| !h.is_empty()) {
        value["sha1"] = hash_file(path)?.into();
    }
    validate_hash(value["sha1"].as_str().ok_or("依赖校验值无效。")?)?;
    if !value["url"].is_string() {
        value["url"] = "".into();
    }
    Ok(())
}

pub fn prepare(root: &Path, id: &str, platform: &Platform) -> Result<(Version, PathBuf)> {
    describe(root, id)?;
    let (mut value, client) = profile(root, id, &mut BTreeSet::new())?;
    let raw_libraries = value["libraries"]
        .as_array()
        .ok_or("版本缺少依赖清单。")?
        .clone();
    let mut active = Vec::new();
    for mut library in raw_libraries {
        let rules: Vec<super::model::Rule> = serde_json::from_value(
            library["rules"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into(),
        )
        .map_err(|_| "依赖规则无效。")?;
        if !super::install::rules_allow(&rules, platform, false) {
            continue;
        }
        let relative =
            super::loaders::maven_path(library["name"].as_str().ok_or("依赖缺少名称。")?)?;
        if !library["downloads"]["artifact"].is_object() && library["natives"].is_null() {
            let sha1 = library["sha1"].clone();
            let size = library["size"].clone();
            library["downloads"] = json!({"artifact":{"path":relative,"sha1":sha1,"size":size}});
        }
        if library["downloads"]["artifact"].is_object() {
            let entry = &mut library["downloads"]["artifact"];
            if entry["path"].is_null() {
                entry["path"] = relative.into();
            }
            let path = safe_join(
                &root.join("libraries"),
                entry["path"].as_str().ok_or("依赖路径无效。")?,
            )?;
            artifact(entry, &path)?;
        }
        let classifier = library["natives"][&platform.os]
            .as_str()
            .map(|s| s.replace("${arch}", if platform.arch == "x86" { "32" } else { "64" }));
        if let Some(classifiers) = library
            .get_mut("downloads")
            .and_then(|d| d.get_mut("classifiers"))
            .and_then(Value::as_object_mut)
        {
            classifiers.retain(|name, _| classifier.as_ref() == Some(name));
            for (name, entry) in classifiers {
                if !entry["path"].is_string() {
                    return Err(format!("原生库 {name} 缺少路径。"));
                }
                let path = safe_join(&root.join("libraries"), entry["path"].as_str().unwrap())?;
                artifact(entry, &path)?;
            }
        }
        if let Some(downloads) = library.get_mut("downloads").and_then(Value::as_object_mut)
            && downloads.get("classifiers").is_some_and(Value::is_null)
        {
            downloads.remove("classifiers");
        }
        for key in ["rules", "natives"] {
            if library.get(key).is_some_and(Value::is_null) {
                library.as_object_mut().unwrap().remove(key);
            }
        }
        active.push(library);
    }
    value["libraries"] = Value::Array(active);
    artifact(&mut value["downloads"]["client"], &client)?;
    let index = safe_join(
        &root.join("assets/indexes"),
        &format!(
            "{}.json",
            value["assetIndex"]["id"]
                .as_str()
                .ok_or("资源索引缺少名称。")?
        ),
    )?;
    artifact(&mut value["assetIndex"], &index)?;
    if !value["logging"]["client"].is_object() {
        value["logging"] = Value::Null;
    }
    if value["logging"]["client"]["file"].is_object() {
        let file = &mut value["logging"]["client"]["file"];
        let candidates = super::logging::paths(
            root,
            id,
            &client,
            None,
            file["id"].as_str().ok_or("日志配置缺少名称。")?,
        )?;
        let path = candidates
            .iter()
            .find(|path| fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0))
            .unwrap_or(&candidates[0]);
        artifact(file, path)?;
    }
    let version =
        serde_json::from_value(value).map_err(|e| format!("已有版本启动配置不兼容：{e}"))?;
    Ok((version, client))
}

#[cfg(test)]
mod tests;
