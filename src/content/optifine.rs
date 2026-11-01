use super::{LocalKind, Target, import_local};
use crate::{
    instances::Instance,
    minecraft::{Downloader, LoaderKind, Result},
};
use std::{fs, path::Path};
pub fn import_optifine(
    root: &Path,
    instance: &Instance,
    path: &Path,
    confirmed_forge: &str,
    d: &Downloader,
) -> Result<String> {
    let target = Target::from_instance(root, instance)?;
    if target.loader != Some(LoaderKind::Forge) {
        return Err(
            "此安装方式仅用于 Forge 实例；其他加载器请使用各自支持的渲染/光影模组。".into(),
        );
    }
    if target.loader_version.as_deref() != Some(confirmed_forge) || confirmed_forge.is_empty() {
        return Err("Forge 版本已改变，请重新核对 OptiFine 官方下载页的兼容说明。".into());
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("OptiFine 文件名无效。")?;
    let name = name.strip_prefix("preview_").unwrap_or(name);
    if !name.starts_with(&format!("OptiFine_{}_HD_", target.minecraft)) || !name.ends_with(".jar") {
        return Err("请选择保留官方文件名且匹配此 Minecraft 版本的 OptiFine JAR。".into());
    }
    let zip = zip::ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?)
        .map_err(|_| "OptiFine JAR 损坏。")?;
    if !zip.file_names().any(|n| n == "optifine/Installer.class") {
        return Err("此文件未包含 OptiFine 安装器标记。".into());
    }
    import_local(root, instance, LocalKind::Mods, &[path.to_path_buf()], d)
}
