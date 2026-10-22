use crate::{
    minecraft::{self, Result},
    store::Settings,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const CONFIG: &str = "nscl-instance.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub version_id: String,
    pub java_path: Option<String>,
    pub memory: Option<u32>,
    pub last_played: Option<u64>,
}
impl Instance {
    pub fn directory(&self, root: &Path) -> Result<PathBuf> {
        managed_path(root, &self.id)
    }
    pub fn effective_settings(&self, settings: &Settings) -> Settings {
        let mut result = settings.clone();
        if let Some(path) = &self.java_path {
            result.java_path = path.clone();
        }
        if let Some(memory) = self.memory {
            result.memory = memory;
        }
        result
    }
    pub fn validate(&self) -> Result<()> {
        minecraft::validate_id(&self.id)?;
        minecraft::validate_id(&self.version_id)?;
        if self.name.trim().is_empty()
            || self.name.chars().count() > 64
            || self.name.chars().any(char::is_control)
        {
            return Err("实例名称应为 1–64 个可显示字符。".into());
        }
        if self.memory.is_some_and(|m| !(1024..=32768).contains(&m)) {
            return Err("实例内存必须为 1024–32768 MiB。".into());
        }
        Ok(())
    }
}

fn reject_link(path: &Path) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("实例目录不能是符号链接或目录联接。".into());
            }
        }
        if meta.file_type().is_symlink() {
            return Err("实例目录不能是符号链接。".into());
        }
    }
    Ok(())
}
fn managed_path(root: &Path, id: &str) -> Result<PathBuf> {
    minecraft::validate_id(id)?;
    let parent = root.join("instances");
    reject_link(&parent)?;
    let path = parent.join(id);
    reject_link(&path)?;
    Ok(path)
}

pub fn list(root: &Path) -> Vec<Instance> {
    let installed = minecraft::installed_versions(root);
    let mut result = Vec::new();
    let parent = root.join("instances");
    if reject_link(&parent).is_err() {
        return result;
    }
    if let Ok(entries) = fs::read_dir(&parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if reject_link(&path).is_err() {
                continue;
            }
            if let Ok(bytes) = fs::read(path.join(CONFIG))
                && let Ok(instance) = serde_json::from_slice::<Instance>(&bytes)
                && instance.validate().is_ok()
                && entry.file_name() == instance.id.as_str()
            {
                result.push(instance);
            }
        }
    }
    // Existing installations retain their original saves and instance directory.
    for version in installed {
        if !result.iter().any(|i| i.id == version.id)
            && !parent.join(format!("{}.archived", version.id)).exists()
            && managed_path(root, &version.id).is_ok()
        {
            result.push(Instance {
                id: version.id.clone(),
                name: format!("Minecraft {}", version.id),
                version_id: version.id,
                ..Default::default()
            });
        }
    }
    result.sort_by(|a, b| b.last_played.cmp(&a.last_played).then(a.name.cmp(&b.name)));
    result
}

pub fn save(root: &Path, instance: &Instance) -> Result<()> {
    instance.validate()?;
    let directory = instance.directory(root)?;
    fs::create_dir_all(&directory).map_err(|e| format!("无法创建实例目录：{e}"))?;
    reject_link(&directory.join(CONFIG))?;
    crate::store::write_json(&directory.join(CONFIG), instance)
}

pub fn create(root: &Path, version_id: &str, name: &str) -> Result<Instance> {
    if !minecraft::installed_versions(root)
        .iter()
        .any(|v| v.id == version_id)
    {
        return Err("该版本尚未完成安装，请先在下载页安装。".into());
    }
    let instance = Instance {
        id: format!("instance-{}", uuid::Uuid::new_v4().simple()),
        version_id: version_id.into(),
        name: name.trim().into(),
        ..Default::default()
    };
    save(root, &instance)?;
    Ok(instance)
}

/// Move the whole game directory to a recoverable archive; shared versions/assets are retained.
pub fn archive(root: &Path, instance: &Instance) -> Result<PathBuf> {
    instance.validate()?;
    let source = instance.directory(root)?;
    if !source.exists() {
        fs::create_dir_all(&source).map_err(|e| e.to_string())?;
    }
    // Persist legacy instances so they can be restored with their settings intact.
    save(root, instance)?;
    let parent = root.join("archived-instances");
    reject_link(&parent)?;
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let target = parent.join(format!("archive-{}", uuid::Uuid::new_v4().simple()));
    let hidden = root
        .join("instances")
        .join(format!("{}.archived", instance.id));
    if instance.id == instance.version_id {
        crate::store::write_json(&hidden, &true)?;
    }
    if let Err(error) = fs::rename(&source, &target) {
        if instance.id == instance.version_id {
            let _ = fs::remove_file(hidden);
        }
        return Err(format!("移入归档失败：{error}"));
    }
    Ok(target)
}

pub fn archived(root: &Path) -> Vec<(String, Instance)> {
    let parent = root.join("archived-instances");
    if reject_link(&parent).is_err() {
        return Vec::new();
    }
    fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            minecraft::validate_id(&name).ok()?;
            reject_link(&entry.path()).ok()?;
            let instance: Instance =
                serde_json::from_slice(&fs::read(entry.path().join(CONFIG)).ok()?).ok()?;
            instance.validate().ok()?;
            Some((name, instance))
        })
        .collect()
}

pub fn restore(root: &Path, entry: &str) -> Result<Instance> {
    minecraft::validate_id(entry)?;
    let parent = root.join("archived-instances");
    reject_link(&parent)?;
    let source = parent.join(entry);
    reject_link(&source)?;
    let instance: Instance =
        serde_json::from_slice(&fs::read(source.join(CONFIG)).map_err(|e| e.to_string())?)
            .map_err(|_| "归档实例配置损坏。")?;
    instance.validate()?;
    let target = instance.directory(root)?;
    if target.exists() {
        return Err("同名实例目录已存在，请先处理目录冲突。".into());
    }
    fs::create_dir_all(root.join("instances")).map_err(|e| e.to_string())?;
    fs::rename(source, target).map_err(|e| format!("恢复实例失败：{e}"))?;
    if instance.id == instance.version_id {
        let _ = fs::remove_file(
            root.join("instances")
                .join(format!("{}.archived", instance.id)),
        );
    }
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> tempfile::TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }
    fn installed(root: &Path) {
        let dir = root.join("versions/1.20.1");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1.20.1.json"), "{}").unwrap();
        crate::store::write_json(
            &dir.join(CONFIG),
            &minecraft::InstalledVersion {
                id: "1.20.1".into(),
                kind: "release".into(),
                java_major: 17,
                platform: "test".into(),
                metadata_sha1: "test".into(),
                loader: None,
                extra_files: Vec::new(),
            },
        )
        .unwrap();
    }
    #[test]
    fn instances_share_version_but_keep_separate_settings_and_saves() {
        let dir = root();
        installed(dir.path());
        let mut one = create(dir.path(), "1.20.1", "生存世界").unwrap();
        let two = create(dir.path(), "1.20.1", "测试世界").unwrap();
        one.memory = Some(2048);
        one.java_path = Some(String::new());
        save(dir.path(), &one).unwrap();
        assert_ne!(
            one.directory(dir.path()).unwrap(),
            two.directory(dir.path()).unwrap()
        );
        assert_eq!(one.effective_settings(&Settings::default()).memory, 2048);
        assert_eq!(two.effective_settings(&Settings::default()).memory, 4096);
        fs::write(
            one.directory(dir.path()).unwrap().join("save-data"),
            "world",
        )
        .unwrap();
        let archive = archive(dir.path(), &one).unwrap();
        assert!(archive.join("save-data").exists());
        assert!(dir.path().join("versions/1.20.1/1.20.1.json").exists());
        assert!(!list(dir.path()).iter().any(|i| i.id == one.id));
        let entry = archive.file_name().unwrap().to_str().unwrap();
        let restored = restore(dir.path(), entry).unwrap();
        assert_eq!(restored.id, one.id);
        assert!(
            restored
                .directory(dir.path())
                .unwrap()
                .join("save-data")
                .exists()
        );
        assert!(create(dir.path(), "missing-version", "test").is_err());
    }
    #[test]
    fn paths_and_invalid_memory_are_rejected() {
        let instance = Instance {
            id: "../outside".into(),
            name: "test".into(),
            version_id: "1.20.1".into(),
            ..Default::default()
        };
        assert!(instance.directory(Path::new("root")).is_err());
        let instance = Instance {
            id: "test".into(),
            memory: Some(0),
            ..instance
        };
        assert!(instance.validate().is_err());
    }

    #[test]
    fn archiving_legacy_default_does_not_create_an_empty_replacement() {
        let dir = root();
        installed(dir.path());
        let instance = list(dir.path()).pop().unwrap();
        let path = archive(dir.path(), &instance).unwrap();
        assert!(list(dir.path()).is_empty());
        restore(dir.path(), path.file_name().unwrap().to_str().unwrap()).unwrap();
        assert_eq!(list(dir.path()).len(), 1);
    }

    #[test]
    fn missing_shared_client_does_not_hide_or_prevent_archiving_saved_instances() {
        let dir = root();
        installed(dir.path());
        let instance = create(dir.path(), "1.20.1", "保存我的世界").unwrap();
        fs::remove_file(dir.path().join("versions/1.20.1/1.20.1.json")).unwrap();
        assert!(list(dir.path()).iter().any(|i| i.id == instance.id));
        let path = archive(dir.path(), &instance).unwrap();
        assert!(restore(dir.path(), path.file_name().unwrap().to_str().unwrap()).is_ok());
    }
}
