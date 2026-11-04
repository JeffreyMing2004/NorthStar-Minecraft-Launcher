use super::*;

#[cfg(test)]
#[path = "deletion_tests.rs"]
mod tests;

// Verify the resolved scope and all descendants before recursively deleting.
// In particular, never traverse directory junctions inside an instance.
fn validate_tree(root: &Path, container: &Path, target: &Path) -> Result<()> {
    reject_link(root)?;
    reject_link(container)?;
    reject_link(target)?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let container = fs::canonicalize(container).map_err(|e| e.to_string())?;
    let resolved = fs::canonicalize(target).map_err(|e| e.to_string())?;
    if container == root
        || !container.starts_with(&root)
        || resolved == container
        || !resolved.starts_with(&container)
    {
        return Err("实例删除路径超出所属目录。".into());
    }
    let mut pending = vec![target.to_path_buf()];
    while let Some(path) = pending.pop() {
        reject_link(&path)?;
        if fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            for entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
                pending.push(entry.map_err(|e| e.to_string())?.path());
            }
        }
    }
    Ok(())
}

/// Delete the owned instance directory; shared runtimes and game data stay in place.
pub fn delete(root: &Path, instance: &Instance) -> Result<()> {
    instance.validate()?;
    let current = list(root)
        .into_iter()
        .find(|i| i.id == instance.id)
        .ok_or("实例不存在，请刷新列表。")?;
    if current != *instance {
        return Err("实例配置已变化，请重新选择后删除。".into());
    }
    let target = instance.directory(root)?;
    if target.exists() {
        validate_tree(root, &root.join("instances"), &target)?;
    } else {
        reject_link(root)?;
    }
    // Legacy/default instances are synthesized from installed runtime metadata.
    // Keep a small marker so refreshing does not recreate a deleted instance.
    let hidden = root
        .join("instances")
        .join(format!("{}.deleted", instance.id));
    let create_marker = instance.is_default() && !hidden.exists();
    if create_marker {
        reject_link(&hidden)?;
        crate::store::write_json(&hidden, &true)?;
    }
    if target.exists()
        && let Err(error) = fs::remove_dir_all(&target)
    {
        if create_marker {
            let _ = fs::remove_file(&hidden);
        }
        return Err(format!("删除实例失败，部分文件可能已删除：{error}"));
    }
    Ok(())
}

pub fn delete_archived(root: &Path, entry: &str, expected: &Instance) -> Result<()> {
    minecraft::validate_id(entry)?;
    expected.validate()?;
    let container = root.join("archived-instances");
    let target = container.join(entry);
    validate_tree(root, &container, &target)?;
    let current: Instance =
        serde_json::from_slice(&fs::read(target.join(CONFIG)).map_err(|e| e.to_string())?)
            .map_err(|_| "归档实例配置损坏。")?;
    if current != *expected {
        return Err("归档实例已变化，请刷新后重新选择。".into());
    }
    fs::remove_dir_all(&target).map_err(|e| format!("删除归档实例失败，部分文件可能已删除：{e}"))
}
