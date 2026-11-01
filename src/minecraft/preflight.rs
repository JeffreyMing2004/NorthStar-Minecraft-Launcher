use super::Result;
use std::{fs, path::Path};
pub fn directory(path: &Path) -> Result<()> {
    for p in path.ancestors() {
        crate::instances::reject_link(p)?;
    }
    fs::create_dir_all(path).map_err(|e| format!("游戏目录不可用：{e}"))?;
    let probe = path.join(format!(
        ".nscl-write-check-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|e| format!("游戏目录不可写：{e}"))?;
    drop(file);
    fs::remove_file(probe).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available = 0u64;
        // SAFETY: buffers remain live for this synchronous filesystem query.
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } != 0
            && available < 512 * 1024 * 1024
        {
            return Err(
                "游戏目录所在磁盘可用空间不足 512 MiB，请清理空间或更换目录后启动。".into(),
            );
        }
    }
    Ok(())
}
