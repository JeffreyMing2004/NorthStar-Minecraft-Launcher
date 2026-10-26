use crate::content::LocalKind;
use std::path::PathBuf;

pub fn resources(kind: LocalKind) -> Result<Vec<PathBuf>, String> {
    #[cfg(windows)]
    {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        use windows_sys::Win32::UI::Controls::Dialogs::{
            CommDlgExtendedError, GetOpenFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER,
            OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OPENFILENAMEW,
        };
        let filter: Vec<u16> = (if kind == LocalKind::Mods {
            "模组 JAR\0*.jar\0\0"
        } else {
            "资源 ZIP\0*.zip\0\0"
        })
        .encode_utf16()
        .collect();
        let title: Vec<u16> = format!("导入{}\0", kind.label()).encode_utf16().collect();
        let mut buffer = vec![0u16; 65536];
        let mut options = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            lpstrFilter: filter.as_ptr(),
            nFilterIndex: 1,
            lpstrTitle: title.as_ptr(),
            lpstrFile: buffer.as_mut_ptr(),
            nMaxFile: buffer.len() as u32,
            Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_ALLOWMULTISELECT | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        // SAFETY: pointers refer to live UTF-16 buffers with the capacities given above.
        if unsafe { GetOpenFileNameW(&mut options) } == 0 {
            // SAFETY: this API reports the last dialog error on the calling thread.
            let error = unsafe { CommDlgExtendedError() };
            return if error == 0 {
                Ok(vec![])
            } else {
                Err(format!("打开文件选择器失败：{error}"))
            };
        }
        let parts: Vec<_> = buffer
            .split(|&c| c == 0)
            .take_while(|p| !p.is_empty())
            .map(OsString::from_wide)
            .collect();
        let Some(first) = parts.first() else {
            return Ok(vec![]);
        };
        if parts.len() == 1 {
            return Ok(vec![PathBuf::from(first)]);
        }
        Ok(parts[1..]
            .iter()
            .map(|p| PathBuf::from(first).join(p))
            .collect())
    }
    #[cfg(not(windows))]
    {
        let _ = kind;
        Err("请将本地文件拖入此资源管理页进行导入。".into())
    }
}
