use super::{Downloader, LoaderKind, LoaderVersion, Result, validate_id};
use serde::Deserialize;

#[derive(Deserialize)]
struct MavenMetadata {
    versioning: MavenVersioning,
}
#[derive(Deserialize)]
struct MavenVersioning {
    versions: MavenVersions,
}
#[derive(Deserialize)]
struct MavenVersions {
    #[serde(rename = "version", default)]
    values: Vec<String>,
}
#[derive(Deserialize)]
struct FabricEntry {
    loader: FabricLoader,
}
#[derive(Deserialize)]
struct FabricLoader {
    version: String,
    stable: bool,
}

pub fn loader_versions(
    minecraft: &str,
    kind: LoaderKind,
    downloader: &Downloader,
) -> Result<Vec<LoaderVersion>> {
    validate_id(minecraft)?;
    downloader.phase(
        &format!("获取 Minecraft {minecraft} 的 {} 版本", kind.label()),
        0,
    );
    let mut versions = if kind == LoaderKind::Fabric {
        let entries: Vec<FabricEntry> = downloader.json(&format!(
            "https://meta.fabricmc.net/v2/versions/loader/{minecraft}"
        ))?;
        entries
            .into_iter()
            .map(|entry| LoaderVersion {
                kind,
                minecraft: minecraft.into(),
                version: entry.loader.version,
                stable: entry.loader.stable,
            })
            .collect()
    } else {
        let url = match kind {
            LoaderKind::Forge => {
                "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml"
            }
            LoaderKind::NeoForge if minecraft == "1.20.1" => {
                "https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml"
            }
            LoaderKind::NeoForge => {
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml"
            }
            _ => unreachable!(),
        };
        downloader.fetch(url, |bytes| {
            let xml = std::str::from_utf8(bytes).map_err(|_| "加载器版本列表编码无效。")?;
            parse_maven(xml, minecraft, kind)
        })?
    };
    versions.retain(|version| version.id().is_ok());
    versions.sort_by(|left, right| {
        numbers(&right.version)
            .cmp(&numbers(&left.version))
            .then(right.version.cmp(&left.version))
    });
    versions.dedup_by(|left, right| left.version == right.version);
    Ok(versions)
}

pub(super) fn parse_maven(
    xml: &str,
    minecraft: &str,
    kind: LoaderKind,
) -> Result<Vec<LoaderVersion>> {
    let metadata: MavenMetadata =
        quick_xml::de::from_str(xml).map_err(|e| format!("加载器 Maven 列表无效：{e}"))?;
    let prefix = format!("{minecraft}-");
    Ok(metadata
        .versioning
        .versions
        .values
        .into_iter()
        .filter_map(|raw| {
            let version = if kind == LoaderKind::Forge || minecraft == "1.20.1" {
                raw.strip_prefix(&prefix)?.to_owned()
            } else {
                if neoforge_minecraft(&raw).as_deref() != Some(minecraft) {
                    return None;
                }
                raw
            };
            let lower = version.to_ascii_lowercase();
            let stable = !["alpha", "beta", "snapshot", "rc"]
                .iter()
                .any(|tag| lower.contains(tag));
            Some(LoaderVersion {
                kind,
                minecraft: minecraft.into(),
                version,
                stable,
            })
        })
        .collect())
}

fn numbers(value: &str) -> Vec<u64> {
    value
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse().ok())
        .collect()
}

fn neoforge_minecraft(version: &str) -> Option<String> {
    let core = version.split('-').next()?;
    let parts: Vec<u32> = core
        .split('.')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()
        .ok()?;
    if parts.len() < 3 {
        return None;
    }
    let (major, minor) = (parts[0], parts[1]);
    if major < 26 {
        Some(if minor == 0 {
            format!("1.{major}")
        } else {
            format!("1.{major}.{minor}")
        })
    } else if parts.len() >= 4 {
        Some(if parts[2] == 0 {
            format!("{major}.{minor}")
        } else {
            format!("{major}.{minor}.{}", parts[2])
        })
    } else {
        None
    }
}
