use crate::minecraft::Result;

#[derive(Clone, Copy, Debug)]
pub struct MemoryStatus {
    pub total_mib: u64,
    pub available_mib: u64,
}

pub fn status() -> Option<MemoryStatus> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut info = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: info is a writable, correctly sized MEMORYSTATUSEX structure.
        if unsafe { GlobalMemoryStatusEx(&mut info) } != 0 {
            return Some(MemoryStatus {
                total_mib: info.ullTotalPhys / (1024 * 1024),
                available_mib: info.ullAvailPhys / (1024 * 1024),
            });
        }
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let mib = |key: &str| {
            text.lines().find_map(|line| {
                line.strip_prefix(key)?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
                    .map(|kib| kib / 1024)
            })
        };
        return Some(MemoryStatus {
            total_mib: mib("MemTotal:")?,
            available_mib: mib("MemAvailable:")?,
        });
    }
    #[allow(unreachable_code)]
    None
}

pub fn automatic(java_major: u32, modded: bool) -> Result<u32> {
    match status() {
        Some(status) => recommend(status, java_major, modded),
        // Keep a conservative heap when the OS cannot report physical memory.
        None => Ok(2048),
    }
}

pub fn recommend(status: MemoryStatus, java_major: u32, modded: bool) -> Result<u32> {
    let desired = match (java_major >= 17, modded) {
        (true, true) => 6144,
        (true, false) | (false, true) => 4096,
        (false, false) => 2048,
    };
    // Leave at least 1 GiB available to the OS and never give the heap more than
    // half of physical RAM. Round down so the recommendation cannot exceed it.
    let budget = (status.total_mib / 2).min(status.available_mib.saturating_sub(1024));
    if budget < 1024 {
        return Err(
            "可用物理内存不足，无法安全分配至少 1024 MiB 游戏内存，请关闭部分程序后重试。".into(),
        );
    }
    Ok((desired.min(budget) / 256 * 256) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_heap_reserves_memory_and_accounts_for_modded_games() {
        let desktop = MemoryStatus {
            total_mib: 16384,
            available_mib: 12000,
        };
        assert_eq!(recommend(desktop, 17, false).unwrap(), 4096);
        assert_eq!(recommend(desktop, 17, true).unwrap(), 6144);
        assert_eq!(recommend(desktop, 8, false).unwrap(), 2048);
        let busy = MemoryStatus {
            total_mib: 8192,
            available_mib: 3000,
        };
        let heap = recommend(busy, 21, true).unwrap();
        assert!(u64::from(heap) + 1024 <= busy.available_mib);
        assert!(u64::from(heap) <= busy.total_mib / 2);
        assert!(
            recommend(
                MemoryStatus {
                    available_mib: 1500,
                    ..busy
                },
                21,
                true
            )
            .is_err()
        );
    }
}
