use crate::content::LocalKind;
use std::path::PathBuf;

pub fn frpc() -> Result<Option<PathBuf>, String> {
    #[cfg(windows)]
    {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        use windows_sys::Win32::UI::Controls::Dialogs::{
            CommDlgExtendedError, GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST,
            OFN_NOCHANGEDIR, OPENFILENAMEW,
        };
        let filter: Vec<u16> = "frpc 程序\0frpc.exe\0可执行文件\0*.exe\0\0"
            .encode_utf16()
            .collect();
        let title: Vec<u16> = "选择 frpc 程序\0".encode_utf16().collect();
        let mut buffer = vec![0u16; 32768];
        let mut options = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            lpstrFilter: filter.as_ptr(),
            lpstrTitle: title.as_ptr(),
            nFilterIndex: 1,
            lpstrFile: buffer.as_mut_ptr(),
            nMaxFile: buffer.len() as u32,
            Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        // SAFETY: all pointers refer to live UTF-16 buffers of the declared capacities.
        if unsafe { GetOpenFileNameW(&mut options) } == 0 {
            // SAFETY: retrieves the dialog error for this calling thread.
            let error = unsafe { CommDlgExtendedError() };
            return if error == 0 {
                Ok(None)
            } else {
                Err(format!("打开 frpc 选择器失败：{error}"))
            };
        }
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Ok(Some(PathBuf::from(OsString::from_wide(&buffer[..end]))))
    }
    #[cfg(not(windows))]
    {
        Err("请直接填写本地 frpc 程序路径。".into())
    }
}

pub fn save_pack(filename: &str) -> Result<Option<PathBuf>, String> {
    save_as(
        filename,
        "Modrinth 整合包\0*.mrpack\0\0",
        "导出整合包（保存为新文件）\0",
        "mrpack\0",
    )
}

pub fn save_diagnostics() -> Result<Option<PathBuf>, String> {
    save_as(
        "nscl-diagnostics.zip",
        "ZIP 诊断包\0*.zip\0\0",
        "导出诊断包（保存为新文件）\0",
        "zip\0",
    )
}

fn save_as(
    filename: &str,
    filter: &str,
    title: &str,
    extension: &str,
) -> Result<Option<PathBuf>, String> {
    #[cfg(windows)]
    {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        use windows_sys::Win32::UI::Controls::Dialogs::{
            CommDlgExtendedError, GetSaveFileNameW, OFN_EXPLORER, OFN_NOCHANGEDIR,
            OFN_PATHMUSTEXIST, OPENFILENAMEW,
        };
        let filter: Vec<u16> = filter.encode_utf16().collect();
        let title: Vec<u16> = title.encode_utf16().collect();
        let extension: Vec<u16> = extension.encode_utf16().collect();
        let mut buffer = vec![0u16; 32768];
        for (target, value) in buffer.iter_mut().take(32767).zip(filename.encode_utf16()) {
            *target = value;
        }
        let mut options = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            lpstrFilter: filter.as_ptr(),
            nFilterIndex: 1,
            lpstrTitle: title.as_ptr(),
            lpstrDefExt: extension.as_ptr(),
            lpstrFile: buffer.as_mut_ptr(),
            nMaxFile: buffer.len() as u32,
            Flags: OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        // SAFETY: all pointers refer to live UTF-16 buffers with the capacities given above.
        if unsafe { GetSaveFileNameW(&mut options) } == 0 {
            // SAFETY: reports the last dialog error on the calling thread.
            let error = unsafe { CommDlgExtendedError() };
            return if error == 0 {
                Ok(None)
            } else {
                Err(format!("打开保存对话框失败：{error}"))
            };
        }
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Ok(Some(PathBuf::from(OsString::from_wide(&buffer[..end]))))
    }
    #[cfg(not(windows))]
    {
        let _ = (filename, filter, title, extension);
        Err("当前系统不支持保存文件对话框。".into())
    }
}

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
