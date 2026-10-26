//! Select a graphics backend before window creation and keep startup diagnostics available.
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Once, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

static LOGGER: OnceLock<GraphicsLog> = OnceLock::new();
static FIRST_FRAME: Once = Once::new();
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
#[cfg(debug_assertions)]
static CAPTURE_PATH: OnceLock<PathBuf> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Glow,
    Wgpu,
    Software,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Glow => "glow",
            Self::Wgpu => "wgpu",
            Self::Software => "software",
        })
    }
}

fn default_mode(vmware: bool) -> Mode {
    if cfg!(windows) && vmware {
        Mode::Software
    } else if cfg!(windows) {
        Mode::Glow
    } else {
        Mode::Wgpu
    }
}

pub fn select(arguments: impl IntoIterator<Item = OsString>) -> Result<Mode, String> {
    let mut selected = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let value = if argument == "--renderer" {
            Some(
                arguments
                    .next()
                    .ok_or("--renderer 后需要填写 glow、wgpu 或 software。")?,
            )
        } else {
            argument
                .to_str()
                .and_then(|a| a.strip_prefix("--renderer="))
                .map(OsString::from)
        };
        if let Some(value) = value {
            selected = Some(match value.to_str() {
                Some("glow" | "opengl") => Mode::Glow,
                Some("wgpu") => Mode::Wgpu,
                Some("software") => Mode::Software,
                _ => return Err("渲染模式只支持 glow（OpenGL）、wgpu 或 software（CPU）。".into()),
            });
        }
    }
    Ok(selected.unwrap_or_else(|| {
        let vmware = vmware_guest();
        if vmware {
            record("VMware firmware detected; selecting Windows CPU software renderer");
        }
        default_mode(vmware)
    }))
}

#[cfg(any(windows, test))]
fn has_vmware_marker(bytes: &[u8]) -> bool {
    bytes
        .windows(6)
        .any(|word| word.eq_ignore_ascii_case(b"vmware"))
}

#[cfg(windows)]
fn vmware_guest() -> bool {
    use windows_sys::Win32::System::SystemInformation::GetSystemFirmwareTable;
    const PROVIDER: u32 = u32::from_be_bytes(*b"RSMB");
    // Query size first, then read into an initialized and bounded buffer. Firmware is read-only.
    let size = unsafe { GetSystemFirmwareTable(PROVIDER, 0, std::ptr::null_mut(), 0) };
    if size == 0 || size > 1024 * 1024 {
        return false;
    }
    let mut bytes = vec![0; size as usize];
    let written = unsafe { GetSystemFirmwareTable(PROVIDER, 0, bytes.as_mut_ptr().cast(), size) };
    written > 0 && written <= size && has_vmware_marker(&bytes[..written as usize])
}
#[cfg(not(windows))]
fn vmware_guest() -> bool {
    false
}

pub fn configure(
    mode: Mode,
    mut options: eframe::NativeOptions,
) -> Result<eframe::NativeOptions, String> {
    options.renderer = match mode {
        Mode::Glow => eframe::Renderer::Glow,
        Mode::Wgpu | Mode::Software => eframe::Renderer::Wgpu,
    };
    if mode == Mode::Software {
        if !cfg!(windows) {
            return Err("CPU 软件渲染目前仅支持 Windows。".into());
        }
        use eframe::{
            egui_wgpu::{WgpuSetup, WgpuSetupCreateNew},
            wgpu,
        };
        let mut setup = WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends = wgpu::Backends::DX12;
        setup.native_adapter_selector = Some(Arc::new(|adapters, surface| {
            adapters.iter().find(|adapter| {
                let info = adapter.get_info();
                info.backend == wgpu::Backend::Dx12
                    && info.device_type == wgpu::DeviceType::Cpu
                    && surface.is_none_or(|surface| adapter.is_surface_supported(surface))
            }).cloned().ok_or_else(|| "未找到可显示窗口的 Windows WARP 软件适配器；未回退到虚拟显卡。请检查 Windows 版本并保留图形日志。".into())
        }));
        options.wgpu_options.wgpu_setup = WgpuSetup::CreateNew(setup);
        record("software mode requires a CPU DX12 adapter; hardware adapters are excluded");
        #[cfg(debug_assertions)]
        if let Some(log) = LOG_PATH.get().and_then(|path| path.parent()) {
            let _ = CAPTURE_PATH.set(log.join("nscl-software-frame.png"));
        }
    }
    Ok(options)
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
        let _ = LOG_PATH.set(path.clone());
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

pub fn record_first_frame(ctx: &egui::Context) {
    FIRST_FRAME.call_once(|| {
        record("first UI frame built (presentation still depends on the graphics driver)");
        #[cfg(debug_assertions)]
        if let Some(path) = CAPTURE_PATH.get() {
            let path = path.clone();
            record("software diagnostic capture requested (application window only)");
            ctx.request_screenshot(move |image| {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                match image::save_buffer(
                    &path,
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => {
                        let colors: std::collections::BTreeSet<_> = image
                            .pixels
                            .iter()
                            .step_by((image.pixels.len() / 4096).max(1))
                            .map(|p| p.to_array())
                            .collect();
                        record(&format!(
                            "software frame saved: {}; size={}x{}; sampled_colors={}",
                            path.display(),
                            image.size[0],
                            image.size[1],
                            colors.len()
                        ));
                    }
                    Err(error) => record(&format!("software frame capture failed: {error}")),
                }
            });
            ctx.request_repaint();
        }
    });
    #[cfg(not(debug_assertions))]
    let _ = ctx;
}

pub fn show_error(error: &str, log_path: Option<&Path>) {
    let log = log_path
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "无法写入启动日志".into());
    let message = format!(
        "启动器界面初始化失败。\n\n{error}\n\n图形日志：{log}\n\n可尝试 --renderer=software（CPU）、--renderer=glow 或 --renderer=wgpu。"
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
        assert_eq!(
            default_mode(false).to_string(),
            if cfg!(windows) { "glow" } else { "wgpu" }
        );
        assert_eq!(
            default_mode(true).to_string(),
            if cfg!(windows) { "software" } else { "wgpu" }
        );
        for (arguments, expected) in [
            (vec!["--renderer=wgpu"], "wgpu"),
            (vec!["--renderer", "glow"], "glow"),
            (vec!["--renderer=opengl"], "glow"),
            (vec!["--renderer=software"], "software"),
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
        assert!(has_vmware_marker(
            b"\0VMware, Inc.\0VMware Virtual Platform\0"
        ));
        assert!(has_vmware_marker(b"\0vmware\0"));
        assert!(!has_vmware_marker(b"\0Physical desktop manufacturer\0"));
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
