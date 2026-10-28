use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[allow(dead_code)] // 发布构建的设置目录会用到
pub const APP_IDENTIFIER: &str = "cool.cnb.jeffreyming.nscl";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub memory: u32,
    pub auto_memory: bool,
    pub java_path: String,
    pub auto_java: bool,
    pub check_updates: bool,
    pub game_directory: String,
    pub game_directories: Vec<String>,
    pub default_isolation: crate::instances::Isolation,
    #[serde(default)]
    pub game_directory_layout: u8,
    pub source: crate::minecraft::Source,
    #[serde(skip_serializing)]
    pub player_name: String,
    #[serde(skip)]
    pub demo_mode: bool,
    pub microsoft_client_id: String,
    pub microsoft_redirect_uri: String,
    pub online_mode: bool,
    pub selected_instance: String,
    pub multiplayer_endpoint: String,
    pub frpc_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            memory: 4096,
            auto_memory: true,
            java_path: String::new(),
            auto_java: true,
            check_updates: true,
            game_directory: String::new(),
            game_directories: Vec::new(),
            default_isolation: crate::instances::Isolation::Isolated,
            game_directory_layout: 1,
            source: crate::minecraft::Source::default(),
            player_name: "NSCLPlayer".into(),
            demo_mode: false,
            microsoft_client_id: crate::accounts::DEFAULT_CLIENT_ID.into(),
            microsoft_redirect_uri: crate::accounts::DEFAULT_REDIRECT_URI.into(),
            online_mode: false,
            selected_instance: String::new(),
            multiplayer_endpoint: crate::multiplayer::frpc::VERIFICATION_ENDPOINTS[0].into(),
            frpc_path: String::new(),
        }
    }
}

impl Settings {
    pub fn configured_java(&self) -> &str {
        if self.auto_java { "" } else { &self.java_path }
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
    let root = if settings.game_directory.trim().is_empty() {
        launcher_directory()
    } else {
        PathBuf::from(settings.game_directory.trim())
    };
    resolve_game_directory(root)
}

fn resolve_game_directory(root: PathBuf) -> PathBuf {
    // An explicitly selected .minecraft is already the game root.
    if root
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(".minecraft"))
    {
        return root;
    }
    let nested = root.join(".minecraft");
    if nested.is_dir() { nested } else { root }
}

pub fn launcher_directory() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn add_game_directory(settings: &mut Settings, path: &str) -> Result<(), String> {
    let path = PathBuf::from(path.trim().trim_matches('"'));
    if path.as_os_str().is_empty() {
        return Err("请填写 Minecraft 游戏目录。".into());
    }
    let path = std::path::absolute(path).map_err(|e| format!("游戏目录无效：{e}"))?;
    std::fs::create_dir_all(&path).map_err(|e| format!("无法创建游戏目录：{e}"))?;
    let path = path.to_string_lossy().into_owned();
    if !settings.game_directories.iter().any(|p| {
        if cfg!(windows) {
            p.eq_ignore_ascii_case(&path)
        } else {
            p == &path
        }
    }) {
        settings.game_directories.push(path.clone());
    }
    settings.game_directory = path;
    settings.game_directory_layout = 1;
    Ok(())
}

fn migrate_game_directory(settings: &mut Settings, legacy: &std::path::Path) {
    if settings.game_directory_layout == 0
        && settings.game_directory.trim().is_empty()
        && (legacy.join("versions").is_dir() || legacy.join("instances").is_dir())
    {
        settings.game_directory = legacy.to_string_lossy().into_owned();
    }
    if !settings.game_directory.trim().is_empty()
        && !settings.game_directories.contains(&settings.game_directory)
    {
        settings
            .game_directories
            .push(settings.game_directory.clone());
    }
    settings.game_directory_layout = 1;
}

pub fn load() -> (Settings, Option<String>) {
    match std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
    {
        Some(mut settings) if (1024..=32768).contains(&settings.memory) => {
            let legacy = settings_path().with_file_name("minecraft");
            migrate_game_directory(&mut settings, &legacy);
            (settings, None)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_root_prefers_minecraft_child_but_never_descends_twice_or_uses_a_file() {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
        std::fs::create_dir_all(&parent).unwrap();
        let root = tempfile::tempdir_in(parent).unwrap();
        let directory = root.path().join("Launcher with spaces");
        std::fs::create_dir_all(&directory).unwrap();
        let mut settings = Settings::default();
        add_game_directory(&mut settings, &directory.to_string_lossy()).unwrap();
        assert_eq!(game_directory(&settings), directory);

        let nested = directory.join(".minecraft");
        std::fs::write(&nested, b"not a directory").unwrap();
        assert_eq!(game_directory(&settings), directory);
        std::fs::remove_file(&nested).unwrap();
        std::fs::create_dir_all(nested.join(".minecraft")).unwrap();
        std::fs::write(directory.join("keep.txt"), b"existing data").unwrap();
        assert_eq!(game_directory(&settings), nested);
        assert_eq!(resolve_game_directory(directory.clone()), nested);
        add_game_directory(&mut settings, &nested.to_string_lossy()).unwrap();
        assert_eq!(game_directory(&settings), nested);
        assert_eq!(
            std::fs::read(directory.join("keep.txt")).unwrap(),
            b"existing data"
        );
    }

    #[test]
    fn default_root_follows_executable_and_custom_paths_are_remembered() {
        let mut settings = Settings::default();
        assert_eq!(game_directory(&settings), launcher_directory());
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
        std::fs::create_dir_all(&parent).unwrap();
        let root = tempfile::tempdir_in(parent).unwrap();
        let path = root.path().join("MC with spaces");
        add_game_directory(&mut settings, &format!("\"{}\"", path.display())).unwrap();
        add_game_directory(&mut settings, &path.to_string_lossy()).unwrap();
        assert_eq!(game_directory(&settings), path);
        assert_eq!(settings.game_directories.len(), 1);
        assert!(path.is_dir());
        assert!(add_game_directory(&mut settings, "  ").is_err());
        let restored: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(restored.game_directories, settings.game_directories);
        settings.game_directory.clear();
        assert_eq!(game_directory(&settings), launcher_directory());
        assert!(path.is_dir());
    }

    #[test]
    fn migration_keeps_existing_games_but_new_profiles_use_launcher_root() {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
        std::fs::create_dir_all(&parent).unwrap();
        let root = tempfile::tempdir_in(parent).unwrap();
        let legacy = root.path().join("minecraft");
        std::fs::create_dir_all(legacy.join("instances")).unwrap();
        let mut old: Settings = serde_json::from_str("{}").unwrap();
        migrate_game_directory(&mut old, &legacy);
        assert_eq!(game_directory(&old), legacy);
        assert_eq!(old.game_directories.len(), 1);
        old.game_directory.clear();
        migrate_game_directory(&mut old, &legacy);
        assert_eq!(game_directory(&old), launcher_directory());
        let mut new = Settings::default();
        migrate_game_directory(&mut new, &legacy);
        assert_eq!(game_directory(&new), launcher_directory());
    }
}
