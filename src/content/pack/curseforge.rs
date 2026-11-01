use super::*;
use crate::content::curseforge::CurseForge;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    manifest_type: String,
    manifest_version: u32,
    name: String,
    version: String,
    minecraft: Game,
    files: Vec<Reference>,
    overrides: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Game {
    version: String,
    mod_loaders: Vec<Component>,
}
#[derive(Deserialize)]
struct Component {
    id: String,
    #[serde(default)]
    primary: bool,
}
#[derive(Deserialize)]
struct Reference {
    #[serde(rename = "projectID")]
    project: u64,
    #[serde(rename = "fileID")]
    file: u64,
    #[serde(default = "required")]
    required: bool,
}
fn required() -> bool {
    true
}
pub(super) fn inspect(root: &Path, path: &Path, d: &Downloader) -> Result<PackPlan> {
    inspect_with_client(root, path, d, &CurseForge::new(d.clone())?)
}
pub(in crate::content) fn inspect_with_client(
    root: &Path,
    path: &Path,
    d: &Downloader,
    api: &CurseForge,
) -> Result<PackPlan> {
    let mut archive = ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if archive.len() > 20000 {
        return Err("整合包压缩条目过多。".into());
    }
    let manifest: Manifest =
        serde_json::from_slice(&archive_entry(&mut archive, "manifest.json", 1024 * 1024)?)
            .map_err(|e| format!("CurseForge manifest.json 无效：{e}"))?;
    if manifest.manifest_type != "minecraftModpack"
        || manifest.manifest_version != 1
        || manifest.files.len() > 10000
    {
        return Err("不支持的 CurseForge 整合包清单。".into());
    }
    minecraft::validate_id(&manifest.minecraft.version)?;
    if manifest.name.is_empty() || manifest.name.len() > 512 || manifest.version.len() > 256 {
        return Err("整合包名称或版本无效。".into());
    }
    minecraft::safe_join(Path::new(""), &manifest.overrides)?;
    let mut dependencies =
        BTreeMap::from([("minecraft".into(), manifest.minecraft.version.clone())]);
    let components: Vec<_> = manifest
        .minecraft
        .mod_loaders
        .iter()
        .filter(|c| c.primary)
        .collect();
    let component = if components.len() == 1 {
        Some(components[0])
    } else if manifest.minecraft.mod_loaders.len() == 1 {
        manifest.minecraft.mod_loaders.first()
    } else if manifest.minecraft.mod_loaders.is_empty() {
        None
    } else {
        return Err("整合包有多个加载器，无法确定主加载器。".into());
    };
    if let Some(c) = component {
        let parsed = [
            ("forge-", "forge"),
            ("neoforge-", "neoforge"),
            ("fabric-loader-", "fabric-loader"),
            ("fabric-", "fabric-loader"),
        ]
        .iter()
        .find_map(|(prefix, key)| c.id.strip_prefix(prefix).map(|v| (*key, v)));
        let (key, value) = parsed.ok_or("CurseForge 包要求尚不支持的加载器。")?;
        minecraft::validate_id(value)?;
        dependencies.insert(key.into(), value.into());
    }
    let mut files_index = Vec::new();
    let mut manual = Vec::new();
    let mut seen = BTreeSet::new();
    for reference in manifest.files {
        d.check_cancelled()?;
        if !seen.insert((reference.project, reference.file)) {
            return Err("CurseForge 清单包含重复文件。".into());
        }
        let mut file = api.file(reference.project, reference.file)?;
        let project = api.project(reference.project)?;
        if project.allow_mod_distribution == Some(false) {
            file.download_url = None;
        }
        let version = file.version()?;
        let data = version.files.first().unwrap();
        minecraft::safe_join(Path::new(""), &data.filename)?;
        if data.filename.contains('/') {
            return Err("文件名不能包含目录。".into());
        }
        let folder = match project.class_id {
            Some(6) => "mods",
            Some(12) => "resourcepacks",
            Some(6552) => "shaderpacks",
            _ => return Err("整合包包含不支持的项目类型。".into()),
        };
        let mut env = BTreeMap::new();
        if !reference.required {
            env.insert("client".into(), "optional".into());
        }
        let urls = if data.url.is_empty() {
            if !files::verified(&files::cache_path(root, data)?, &data.hashes, data.size)? {
                manual.push(version.clone());
            }
            Vec::new()
        } else {
            vec![data.url.clone()]
        };
        files_index.push(PackFile {
            path: format!("{folder}/{}", data.filename),
            hashes: data.hashes.clone(),
            file_size: data.size,
            downloads: urls,
            env,
        });
    }
    let index = PackIndex {
        format_version: 1,
        game: "minecraft".into(),
        version_id: manifest.version,
        name: manifest.name,
        files: files_index,
        dependencies,
    };
    let stage = files::Stage::new(root)?;
    let output = stage.path.join("normalized.mrpack");
    let mut writer = zip::ZipWriter::new(fs::File::create(&output).map_err(|e| e.to_string())?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("modrinth.index.json", options)
        .map_err(|e| e.to_string())?;
    writer
        .write_all(&serde_json::to_vec(&index).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        d.check_cancelled()?;
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().trim_end_matches('/').to_owned();
        minecraft::safe_join(Path::new(""), &name)?;
        if !names.insert(name.to_ascii_lowercase())
            || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("压缩包包含重复路径或符号链接。".into());
        }
        total = total.checked_add(entry.size()).ok_or("压缩包体积溢出。")?;
        if total > 4 * 1024 * 1024 * 1024 || entry.size() > files::MAX_FILE {
            return Err("压缩包解压体积超限。".into());
        }
        if let Some(relative) = name.strip_prefix(&format!("{}/", manifest.overrides))
            && !entry.is_dir()
        {
            pack_path(relative)?;
            writer
                .start_file(format!("overrides/{relative}"), options)
                .map_err(|e| e.to_string())?;
            let mut input = entry.take(files::MAX_FILE + 1);
            let mut buffer = [0; 65536];
            loop {
                d.check_cancelled()?;
                let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                writer.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            }
        }
    }
    writer.finish().map_err(|e| e.to_string())?;
    let mut plan = inspect_modrinth(root, &output, true)?;
    // Keep the normalized verified archive in the regular cache; Stage can now drop.
    plan.manual_files = manual;
    Ok(plan)
}
