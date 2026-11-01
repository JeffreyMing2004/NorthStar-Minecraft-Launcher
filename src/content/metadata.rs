use crate::minecraft::{LoaderKind, Result};
use std::{collections::BTreeMap, fs, io::Read, path::Path};
#[derive(Clone, Debug)]
pub struct Metadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub depends: BTreeMap<String, String>,
    pub breaks: BTreeMap<String, String>,
    pub provides: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}
fn entry(zip: &mut zip::ZipArchive<fs::File>, name: &str) -> Option<String> {
    let file = zip.by_name(name).ok()?;
    if file.size() > 256 * 1024 {
        return None;
    }
    let mut text = String::new();
    file.take(256 * 1024 + 1).read_to_string(&mut text).ok()?;
    if text.len() > 256 * 1024 {
        None
    } else {
        Some(text)
    }
}
pub fn read(path: &Path) -> Result<Vec<Metadata>> {
    read_for(path, None)
}
fn read_for(path: &Path, loader: Option<LoaderKind>) -> Result<Vec<Metadata>> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?)
        .map_err(|_| "不是可读取的模组 JAR。")?;
    if loader == Some(LoaderKind::Quilt)
        && let Some(text) = entry(&mut zip, "quilt.mod.json")
    {
        return quilt(&text).map(|m| vec![m]);
    }
    if let Some(text) = entry(&mut zip, "fabric.mod.json") {
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| "Fabric 模组元数据无效。")?;
        let id = value["id"]
            .as_str()
            .ok_or("Fabric 模组没有 ID。")?
            .to_owned();
        let map = |name: &str| {
            value[name]
                .as_object()
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                if let Some(s) = v.as_str() {
                                    s.into()
                                } else if let Some(values) = v.as_array() {
                                    values
                                        .iter()
                                        .filter_map(|v| v.as_str())
                                        .collect::<Vec<_>>()
                                        .join(" || ")
                                } else {
                                    "?".into()
                                },
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        return Ok(vec![Metadata {
            name: value["name"].as_str().unwrap_or(&id).into(),
            id,
            version: value["version"].as_str().unwrap_or("?").into(),
            depends: map("depends"),
            breaks: map("breaks"),
            provides: value["provides"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
                .map(|id| (id.into(), value["version"].as_str().unwrap_or("?").into()))
                .collect(),
            warnings: Vec::new(),
        }]);
    }
    if let Some(text) = entry(&mut zip, "quilt.mod.json") {
        return quilt(&text).map(|m| vec![m]);
    }
    let text = entry(&mut zip, "META-INF/neoforge.mods.toml")
        .or_else(|| entry(&mut zip, "META-INF/mods.toml"))
        .ok_or("未识别 Fabric / Quilt / Forge / NeoForge 元数据。")?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|_| "Forge / NeoForge TOML 元数据无效。")?;
    let manifest = entry(&mut zip, "META-INF/MANIFEST.MF").unwrap_or_default();
    let implementation = manifest
        .lines()
        .find_map(|l| l.strip_prefix("Implementation-Version:").map(str::trim));
    let mods = value
        .get("mods")
        .and_then(|v| v.as_array())
        .ok_or("TOML 缺少 mods 列表。")?;
    let mut result = Vec::new();
    for m in mods.iter().take(64) {
        let Some(id) = m.get("modId").and_then(|v| v.as_str()) else {
            continue;
        };
        let raw = m.get("version").and_then(|v| v.as_str()).unwrap_or("?");
        let version = if raw == "${file.jarVersion}" {
            implementation.unwrap_or("?")
        } else {
            raw
        };
        let mut depends = BTreeMap::new();
        let mut breaks = BTreeMap::new();
        if let Some(deps) = value
            .get("dependencies")
            .and_then(|d| d.get(id))
            .and_then(|d| d.as_array())
        {
            for dep in deps {
                if dep.get("side").and_then(|v| v.as_str()) == Some("SERVER") {
                    continue;
                }
                let Some(name) = dep.get("modId").and_then(|v| v.as_str()) else {
                    continue;
                };
                let range = dep
                    .get("versionRange")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*");
                if dep.get("type").and_then(|v| v.as_str()) == Some("incompatible") {
                    breaks.insert(name.into(), range.into());
                } else if dep
                    .get("mandatory")
                    .and_then(|v| v.as_bool())
                    .unwrap_or_else(|| dep.get("type").and_then(|v| v.as_str()) != Some("optional"))
                {
                    depends.insert(name.into(), range.into());
                }
            }
        }
        result.push(Metadata {
            id: id.into(),
            name: m
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(id)
                .into(),
            version: version.into(),
            depends,
            breaks,
            provides: BTreeMap::new(),
            warnings: Vec::new(),
        });
    }
    Ok(result)
}
fn quilt(text: &str) -> Result<Metadata> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "Quilt 模组元数据无效。")?;
    if value["schema_version"].as_u64() != Some(1) {
        return Err("尚不支持此 Quilt 元数据版本。".into());
    }
    let q = &value["quilt_loader"];
    let id = q["id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Quilt 模组没有 ID。")?;
    let version = q["version"].as_str().unwrap_or("?");
    let mut result = Metadata {
        id: id.into(),
        name: q["metadata"]["name"].as_str().unwrap_or(id).into(),
        version: version.into(),
        depends: BTreeMap::new(),
        breaks: BTreeMap::new(),
        provides: BTreeMap::new(),
        warnings: Vec::new(),
    };
    for (key, map) in [
        ("depends", &mut result.depends),
        ("breaks", &mut result.breaks),
    ] {
        for dep in q[key].as_array().into_iter().flatten() {
            if dep["optional"].as_bool() == Some(true) {
                continue;
            }
            let name = dep.as_str().or_else(|| dep["id"].as_str());
            if dep.get("unless").is_some() || name.is_none_or(|n| n.contains(':')) {
                result.warnings.push(format!(
                    "{id} 的 Quilt {key} 包含条件或组合依赖，检查范围不完整。"
                ));
                continue;
            }
            let range = match dep.get("versions") {
                None => "*".into(),
                Some(v) if v.is_string() => v.as_str().unwrap().into(),
                Some(v)
                    if v.as_array()
                        .is_some_and(|a| !a.is_empty() && a.iter().all(|x| x.is_string())) =>
                {
                    v.as_array()
                        .unwrap()
                        .iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(" || ")
                }
                _ => {
                    result.warnings.push(format!(
                        "{id} 的 Quilt {key} 包含复杂版本约束，检查范围不完整。"
                    ));
                    continue;
                }
            };
            let name = name.unwrap();
            if map.insert(name.into(), range).is_some() {
                map.insert(name.into(), "?".into());
                result
                    .warnings
                    .push(format!("{id} 对 {name} 声明了多条约束，检查范围不完整。"));
            }
        }
    }
    for provided in q["provides"].as_array().into_iter().flatten() {
        if let Some(name) = provided.as_str().or_else(|| provided["id"].as_str()) {
            result.provides.insert(
                name.into(),
                provided["version"].as_str().unwrap_or(version).into(),
            );
        }
    }
    Ok(result)
}
fn version(text: &str) -> Option<semver::Version> {
    let mut s = text.trim().to_owned();
    if s.is_empty() || s.contains(['$', '?']) {
        return None;
    }
    if !s.contains('-') && !s.contains('+') {
        let count = s.split('.').count();
        if count == 1 {
            s.push_str(".0.0");
        } else if count == 2 {
            s.push_str(".0");
        }
    }
    semver::Version::parse(&s).ok()
}
pub(super) fn audit(
    root: &Path,
    target: &super::Target,
    java: u32,
    report: &mut super::CompatibilityReport,
    d: &crate::minecraft::Downloader,
) -> Result<()> {
    let directory = target.instance.game_directory(root)?;
    let catalog = super::list_local(root, &target.instance, super::LocalKind::Mods, d)?;
    let mut versions = BTreeMap::from([
        ("minecraft".into(), target.minecraft.clone()),
        ("java".into(), java.to_string()),
    ]);
    if let Some(loader) = target.loader {
        versions.insert(
            match loader {
                LoaderKind::Fabric => "fabricloader",
                LoaderKind::Quilt => "quilt_loader",
                _ => loader.key(),
            }
            .into(),
            target.loader_version.clone().unwrap_or_default(),
        );
        if loader == LoaderKind::Quilt {
            // Quilt supplies a Fabric Loader compatibility version, which differs
            // from the Quilt version. Leave its exact version unknown here.
            versions.insert("fabricloader".into(), "?".into());
        }
    }
    let mut mods = Vec::new();
    let active: Vec<_> = catalog
        .entries
        .into_iter()
        .filter(|e| e.enabled && !e.missing && !e.directory)
        .collect();
    if catalog.truncated || active.len() > 256 {
        report
            .warnings
            .push("仅检查前 256 个启用模组的运行时元数据，检查范围不完整。".into());
    }
    for e in active.into_iter().take(256) {
        d.check_cancelled()?;
        match read_for(&directory.join(&e.relative), target.loader) {
            Ok(items) => {
                if items.is_empty() {
                    report
                        .warnings
                        .push(format!("{} 的运行时元数据未知。", e.filename));
                }
                for item in items {
                    report.warnings.extend(item.warnings.iter().cloned());
                    if versions
                        .insert(item.id.clone(), item.version.clone())
                        .is_some()
                    {
                        report
                            .errors
                            .push(format!("模组 ID 重复：{}（{}）。", item.id, e.filename));
                    }
                    mods.push(item);
                }
            }
            Err(_) => report.warnings.push(format!(
                "{} 未提供可解析的 Fabric / Quilt / Forge / NeoForge 元数据，运行时约束未知。",
                e.filename
            )),
        }
    }
    for m in &mods {
        for (id, version) in &m.provides {
            versions
                .entry(id.clone())
                .or_insert_with(|| version.clone());
        }
    }
    for m in mods {
        for (id, range) in m.depends {
            match versions.get(&id) {
                None => report
                    .errors
                    .push(format!("{} 缺少必需依赖 {id} {range}", m.name)),
                Some(v) => match matches(v, &range) {
                    Some(false) => report
                        .errors
                        .push(format!("{} 需要 {id} {range}，当前为 {v}", m.name)),
                    None => report.warnings.push(format!(
                        "{} 的依赖 {id} 约束 {range} 无法完整解析。",
                        m.name
                    )),
                    _ => {}
                },
            }
        }
        for (id, range) in m.breaks {
            if let Some(v) = versions.get(&id) {
                match matches(v, &range) {
                    Some(true) => report
                        .errors
                        .push(format!("{} 与 {id} {range} 冲突。", m.name)),
                    None => report.warnings.push(format!(
                        "{} 的冲突 {id} 约束 {range} 无法完整解析。",
                        m.name
                    )),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
/// None means unsupported syntax; it must be presented as unknown, never compatible.
pub fn matches(actual: &str, range: &str) -> Option<bool> {
    let actual = version(actual)?;
    let range = range.trim();
    if range == "*" || range.is_empty() {
        return Some(true);
    }
    if range.contains("||") {
        let results: Vec<_> = range
            .split("||")
            .map(|r| matches(&actual.to_string(), r))
            .collect();
        if results.contains(&Some(true)) {
            return Some(true);
        }
        return if results.contains(&None) {
            None
        } else {
            Some(false)
        };
    }
    if (range.starts_with('[') || range.starts_with('('))
        && (range.ends_with(']') || range.ends_with(')'))
    {
        let inner = &range[1..range.len() - 1];
        if let Some((low, high)) = inner.split_once(',') {
            if high.contains(',') {
                return None;
            }
            let low = if low.trim().is_empty() {
                true
            } else {
                let v = version(low)?;
                if range.starts_with('[') {
                    actual >= v
                } else {
                    actual > v
                }
            };
            let high = if high.trim().is_empty() {
                true
            } else {
                let v = version(high)?;
                if range.ends_with(']') {
                    actual <= v
                } else {
                    actual < v
                }
            };
            return Some(low && high);
        }
        return Some(actual == version(inner)?);
    }
    if range.split_whitespace().count() > 1 {
        let mut all = true;
        for part in range.split_whitespace() {
            all &= matches(&actual.to_string(), part)?;
        }
        return Some(all);
    }
    if let Some(exact) = version(range) {
        return Some(actual == exact);
    }
    semver::VersionReq::parse(range)
        .ok()
        .map(|r| r.matches(&actual))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_distinguish_exact_maven_and_unknown_constraints() {
        assert_eq!(matches("1.20.1", "[1.20,1.21)"), Some(true));
        assert_eq!(matches("1.21", "[1.20,1.21)"), Some(false));
        assert_eq!(matches("1.2.4", "1.2.3"), Some(false));
        assert_eq!(matches("0.16.2", ">=0.15.0 <0.17.0"), Some(true));
        assert_eq!(matches("1.2.3", "${unknown}"), None);
    }
}
