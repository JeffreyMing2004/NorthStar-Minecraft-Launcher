//! Select a graphics backend before window creation and keep startup diagnostics available.
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, Once, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

static LOGGER: OnceLock<GraphicsLog> = OnceLock::new();
static FIRST_FRAME: Once = Once::new();

pub fn select(arguments: impl IntoIterator<Item = OsString>) -> Result<eframe::Renderer, String> {
    let mut selected = if cfg!(windows) {
        eframe::Renderer::Glow
    } else {
        eframe::Renderer::Wgpu
    };
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let value = if argument == "--renderer" {
            Some(
                arguments
                    .next()
                    .ok_or("--renderer 后需要填写 glow 或 wgpu。")?,
            )
        } else {
            argument
                .to_str()
                .and_then(|a| a.strip_prefix("--renderer="))
                .map(OsString::from)
        };
        if let Some(value) = value {
            selected = match value.to_str() {
                Some("glow" | "opengl") => eframe::Renderer::Glow,
                Some("wgpu") => eframe::Renderer::Wgpu,
                _ => return Err("渲染模式只支持 glow（OpenGL）或 wgpu。".into()),
            };
        }
    }
    Ok(selected)
}

struct GraphicsLog {
    file: Mutex<File>,
}
impl GraphicsLog {
    fn write(&self, level: Level, target: &str, message: &dyn std::fmt::Display) {
        if let Ok(mut file) = self.file.lock() {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let _ = writeln!(file, "{time} {level} {target}: {message}");
            let _ = file.flush();
        }
    }
}
fn graphics_target(target: &str) -> bool {
    [
        "eframe",
        "egui_glow",
        "egui_wgpu",
        "wgpu",
        "glutin",
        "winit",
    ]
    .iter()
    .any(|prefix| {
        target == *prefix
            || target
                .strip_prefix(prefix)
                .is_some_and(|s| s.starts_with("::") || s.starts_with('_'))
    })
}
impl Log for GraphicsLog {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info && graphics_target(metadata.target())
    }
    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            self.write(record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

pub fn initialize_log() -> Option<PathBuf> {
    let primary = nscl::store::settings_path()
        .parent()?
        .join("logs/nscl-startup.log");
    let fallback = std::env::temp_dir().join("NSCL/nscl-startup.log");
    for path in [primary, fallback] {
        let Some(parent) = path.parent() else {
            continue;
        };
        if fs::create_dir_all(parent).is_err() {
            continue;
        }
        let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
            continue;
        };
        let _ = LOGGER.set(GraphicsLog {
            file: Mutex::new(file),
        });
        if let Some(logger) = LOGGER.get() {
            let _ = log::set_logger(logger);
            log::set_max_level(LevelFilter::Info);
            record(&format!(
                "starting NSCL {} on {} {}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH
            ));
        }
        return Some(path);
    }
    None
}

pub fn record(message: &str) {
    if let Some(logger) = LOGGER.get() {
        logger.write(Level::Info, "nscl::graphics", &message);
    }
}

pub fn record_context(context: &eframe::CreationContext<'_>) {
    if let Some(gl) = &context.gl {
        use eframe::glow::HasContext;
        // eframe invokes the creation callback with this OpenGL context current.
        unsafe {
            record(&format!(
                "OpenGL vendor: {}; renderer: {}; version: {}",
                gl.get_parameter_string(eframe::glow::VENDOR),
                gl.get_parameter_string(eframe::glow::RENDERER),
                gl.get_parameter_string(eframe::glow::VERSION)
            ));
        }
    }
    if let Some(state) = &context.wgpu_render_state {
        record(&format!("wgpu adapter: {:?}", state.adapter.get_info()));
    }
    record("graphics context initialized");
}

pub fn record_first_frame() {
    FIRST_FRAME.call_once(|| {
        record("first UI frame built (presentation still depends on the graphics driver)")
    });
}

pub fn show_error(error: &str, log_path: Option<&Path>) {
    let log = log_path
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "无法写入启动日志".into());
    let message = format!(
        "启动器界面初始化失败。\n\n{error}\n\n图形日志：{log}\n\n可尝试使用 --renderer=glow 或 --renderer=wgpu 切换图形后端。"
    );
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let text: Vec<_> = message.encode_utf16().chain([0]).collect();
        let title: Vec<_> = "NSCL 启动失败".encode_utf16().chain([0]).collect();
        // The buffers remain valid and NUL-terminated for the modal call.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    eprintln!("{message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_compatibility_backend_and_accepts_explicit_alternatives() {
        let selected = select([]).unwrap().to_string();
        assert_eq!(selected, if cfg!(windows) { "glow" } else { "wgpu" });
        for (arguments, expected) in [
            (vec!["--renderer=wgpu"], "wgpu"),
            (vec!["--renderer", "glow"], "glow"),
            (vec!["--renderer=opengl"], "glow"),
            (vec!["--renderer=glow", "--renderer=wgpu"], "wgpu"),
        ] {
            assert_eq!(
                select(arguments.into_iter().map(OsString::from))
                    .unwrap()
                    .to_string(),
                expected
            );
        }
        assert!(select([OsString::from("--renderer")]).is_err());
        assert!(select([OsString::from("--renderer=invalid")]).is_err());
    }

    #[test]
    fn diagnostic_log_keeps_graphics_errors_and_excludes_authentication_and_http() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
        fs::create_dir_all(&parent).unwrap();
        let directory = tempfile::tempdir_in(parent).unwrap();
        let path = directory.path().join("graphics.log");
        let logger = GraphicsLog {
            file: Mutex::new(File::create(&path).unwrap()),
        };
        logger.log(
            &Record::builder()
                .target("eframe::native")
                .level(Level::Warn)
                .args(format_args!("graphics failed"))
                .build(),
        );
        logger.log(
            &Record::builder()
                .target("wgpu_hal::dx12")
                .level(Level::Info)
                .args(format_args!("adapter initialized"))
                .build(),
        );
        for target in ["nscl::accounts", "reqwest::connect", "hyper::client"] {
            logger.log(
                &Record::builder()
                    .target(target)
                    .level(Level::Error)
                    .args(format_args!("private-access-token"))
                    .build(),
            );
        }
        logger.log(
            &Record::builder()
                .target("wgpu_core")
                .level(Level::Debug)
                .args(format_args!("debug-only"))
                .build(),
        );
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("graphics failed") && text.contains("adapter initialized"));
        assert!(!text.contains("private-access-token") && !text.contains("debug-only"));
    }
}
