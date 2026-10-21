use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[allow(dead_code)] // 发布构建的设置目录会用到
pub const APP_IDENTIFIER: &str = "cool.cnb.jeffreyming.nscl";

#[derive(Serialize, Deserialize)]
pub struct Settings {
    pub memory: u32,
    pub java_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            memory: 4096,
            java_path: String::new(),
        }
    }
}

// 与 WebView 时代相同的约束：调试构建的可执行文件在部分开发机上无法写入系统盘，
// 调试时设置文件放在可执行文件旁，发布构建使用正式数据目录。
fn settings_path() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("nscl-settings.json")))
            .unwrap_or_else(|| PathBuf::from("nscl-settings.json"))
    }
    #[cfg(not(debug_assertions))]
    {
        let base = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        base.join(APP_IDENTIFIER).join("settings.json")
    }
}

pub fn load() -> (Settings, Option<String>) {
    match std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
    {
        Some(settings) if (1024..=32768).contains(&settings.memory) => (settings, None),
        Some(_) => (
            Settings::default(),
            Some("无法读取本地设置，已使用默认值。".into()),
        ),
        None => (Settings::default(), None),
    }
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = settings_path();
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return Err(format!("保存失败：{e}"));
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| format!("保存失败：{e}"))?;
    std::fs::write(&path, text).map_err(|e| format!("保存失败：{e}"))
}
