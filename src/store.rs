use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[allow(dead_code)] // 发布构建的设置目录会用到
pub const APP_IDENTIFIER: &str = "cool.cnb.jeffreyming.nscl";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub memory: u32,
    pub java_path: String,
    pub game_directory: String,
    pub source: crate::minecraft::Source,
    pub player_name: String,
    pub demo_mode: bool,
    pub microsoft_client_id: String,
    pub microsoft_redirect_uri: String,
    pub online_mode: bool,
    pub selected_instance: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            memory: 4096,
            java_path: String::new(),
            game_directory: String::new(),
            source: crate::minecraft::Source::default(),
            player_name: "NSCLPlayer".into(),
            demo_mode: false,
            microsoft_client_id: crate::accounts::DEFAULT_CLIENT_ID.into(),
            microsoft_redirect_uri: crate::accounts::DEFAULT_REDIRECT_URI.into(),
            online_mode: false,
            selected_instance: String::new(),
        }
    }
}

// 与 WebView 时代相同的约束：调试构建的可执行文件在部分开发机上无法写入系统盘，
// 调试时设置文件放在可执行文件旁，发布构建使用正式数据目录。
pub fn settings_path() -> PathBuf {
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

pub fn game_directory(settings: &Settings) -> PathBuf {
    if settings.game_directory.trim().is_empty() {
        settings_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("minecraft")
    } else {
        PathBuf::from(settings.game_directory.trim())
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
    write_json(&settings_path(), settings)
}

pub fn write_json(path: &std::path::Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return Err(format!("保存失败：{e}"));
    }
    let text = serde_json::to_vec_pretty(value).map_err(|e| format!("保存失败：{e}"))?;
    let temporary = path.with_file_name(format!(".nscl-{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&text)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("保存失败：{e}"))
}
