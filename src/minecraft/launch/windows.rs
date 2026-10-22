use super::Result;
use std::{
    io,
    mem::{size_of, size_of_val},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
use windows_sys::Win32::{
    Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, SECURITY_MAX_SID_SIZE,
        TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub(super) fn is_low_integrity() -> Result<bool> {
    Ok(below_desktop_integrity(process_integrity()?))
}

fn below_desktop_integrity(level: u32) -> bool {
    // SECURITY_MANDATORY_MEDIUM_RID: the level of an ordinary desktop user.
    level < 0x2000
}

fn process_integrity() -> Result<u32> {
    let failure = || format!("读取 Windows 启动权限失败：{}", io::Error::last_os_error());
    let mut raw = std::ptr::null_mut();
    // SAFETY: the pseudo handle is valid, and raw is an output handle. Querying
    // our own token does not change its privileges or impersonate another user.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(failure());
    }
    // SAFETY: OpenProcessToken succeeded and transferred ownership of raw.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    // The returned label includes a pointer to a SID in this same buffer. Use
    // pointer alignment and reserve enough space for the largest Windows SID.
    const WORDS: usize = (size_of::<TOKEN_MANDATORY_LABEL>() + SECURITY_MAX_SID_SIZE as usize)
        .div_ceil(size_of::<usize>());
    let mut buffer = [0usize; WORDS];
    let mut returned = 0;
    // SAFETY: the token remains open, the buffer is aligned for the label, and
    // the byte length describes all of its writable storage.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenIntegrityLevel,
            buffer.as_mut_ptr().cast(),
            size_of_val(&buffer) as u32,
            &mut returned,
        )
    } == 0
    {
        return Err(failure());
    }
    // SAFETY: a successful TokenIntegrityLevel query supplies a valid label and
    // SID; both stay alive in buffer until after the final RID has been copied.
    unsafe {
        let label = &*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let count = *GetSidSubAuthorityCount(label.Label.Sid);
        if count == 0 {
            return Err("Windows 启动权限标识无效。".into());
        }
        Ok(*GetSidSubAuthority(label.Label.Sid, u32::from(count - 1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_integrity_is_distinct_from_normal_desktop() {
        for level in [0, 0x1000] {
            assert!(below_desktop_integrity(level));
        }
    }

    #[test]
    fn normal_desktop_integrity_allows_fabric_launch() {
        for level in [0x2000, 0x2100, 0x3000, 0x4000] {
            assert!(!below_desktop_integrity(level));
        }
    }

    #[test]
    fn current_process_token_is_readable() {
        let level = process_integrity().unwrap();
        assert_eq!(level & 0xff, 0);
        assert!(level <= 0x5000);
    }
}
