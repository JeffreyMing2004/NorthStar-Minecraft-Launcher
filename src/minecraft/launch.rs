use super::{
    Result,
    download::{validate_id, verified},
    install::{
        InstalledVersion, Platform, extract_natives, hidden_command, materialize_assets, plan,
        read_json, rules_allow,
    },
    model::*,
};
use crate::store::Settings;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
    process::Stdio,
};

#[derive(Debug)]
pub struct LaunchPlan {
    pub java: PathBuf,
    pub arguments: Vec<String>,
    pub game_directory: PathBuf,
    pub log_path: PathBuf,
}

pub fn prepare_launch(root: &Path, id: &str, settings: &Settings) -> Result<LaunchPlan> {
    validate_id(id)?;
    validate_player_name(&settings.player_name)?;
    if !(1024..=32768).contains(&settings.memory) {
        return Err("游戏内存必须为 1024–32768 MiB。".into());
    }
    let root = absolute_root(root)?;
    let version_dir = root.join("versions").join(id);
    let installed: InstalledVersion = read_json(&version_dir.join("nscl-instance.json"))?;
    let platform = Platform::current();
    if installed.id != id || installed.platform != platform.key() {
        return Err("该实例未针对当前系统完成安装，请在下载页重新安装。".into());
    }
    let metadata = version_dir.join(format!("{id}.json"));
    if !verified(&metadata, &installed.metadata_sha1, None)? {
        return Err("版本元数据损坏，请重新安装/修复。".into());
    }
    let version: Version = read_json(&metadata)?;
    if version.id != id || version.inherits_from.is_some() {
        return Err("只支持已完整安装的原版客户端。".into());
    }
    validate_id(&version.asset_index.id)?;
    let index_path = root
        .join("assets/indexes")
        .join(format!("{}.json", version.asset_index.id));
    if !verified(
        &index_path,
        &version.asset_index.artifact.sha1,
        version.asset_index.artifact.size,
    )? {
        return Err("资源索引损坏，请重新安装/修复。".into());
    }
    let index: AssetIndex = read_json(&index_path)?;
    let files = plan(&root, &version, &index, &platform)?;
    for job in &files.jobs {
        if !verified(&job.path, &job.sha1, job.size)? {
            return Err(format!(
                "文件缺失或损坏：{}。请在下载页重新安装/修复。",
                job.path.display()
            ));
        }
    }
    let natives = extract_natives(&root, id, &platform, &files.natives, None)?;
    materialize_assets(&root, &version, &index, None)?;
    let java = find_java(&settings.java_path, version.java_major())?;
    let game_directory = root.join("instances").join(id);
    fs::create_dir_all(game_directory.join("logs")).map_err(|e| e.to_string())?;
    let log_path = game_directory.join("logs/nscl-launch.log");
    let arguments = launch_arguments(
        &root,
        &version,
        &index,
        &files.classpath,
        &natives,
        &platform,
        settings,
    )?;
    Ok(LaunchPlan {
        java,
        arguments,
        game_directory,
        log_path,
    })
}

pub fn absolute_root(root: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(root).map_err(|e| format!("游戏目录不可用：{e}"))?;
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{rest}")));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(rest));
        }
    }
    Ok(path)
}

pub fn launch(plan: &LaunchPlan, on_started: impl FnOnce(u32, &Path)) -> Result<i32> {
    let log = File::create(&plan.log_path).map_err(|e| format!("创建游戏日志失败：{e}"))?;
    let stderr = log.try_clone().map_err(|e| e.to_string())?;
    let mut child = hidden_command(&plan.java)
        .args(&plan.arguments)
        .current_dir(&plan.game_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|e| format!("启动 Java 失败：{e}"))?;
    on_started(child.id(), &plan.log_path);
    let status = child.wait().map_err(|e| e.to_string())?;
    Ok(status.code().unwrap_or(-1))
}

pub fn validate_player_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 16
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        Err("离线玩家名必须为 1–16 位英文字母、数字或下划线。".into())
    } else {
        Ok(())
    }
}

pub fn offline_uuid(name: &str) -> String {
    let mut bytes = md5::compute(format!("OfflinePlayer:{name}")).0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn launch_arguments(
    root: &Path,
    version: &Version,
    index: &AssetIndex,
    classpath: &[PathBuf],
    natives: &Path,
    platform: &Platform,
    settings: &Settings,
) -> Result<Vec<String>> {
    let classpath = std::env::join_paths(classpath)
        .map_err(|e| format!("类路径包含非法分隔符：{e}"))?
        .to_string_lossy()
        .into_owned();
    let game_dir = root.join("instances").join(&version.id);
    let assets = root.join("assets");
    let game_assets = if index.virtual_assets {
        assets.join("virtual").join(&version.asset_index.id)
    } else {
        assets.clone()
    };
    let mut values = BTreeMap::from([
        ("auth_player_name", settings.player_name.clone()),
        ("version_name", version.id.clone()),
        ("game_directory", game_dir.to_string_lossy().into_owned()),
        ("assets_root", assets.to_string_lossy().into_owned()),
        ("game_assets", game_assets.to_string_lossy().into_owned()),
        ("assets_index_name", version.asset_index.id.clone()),
        ("auth_uuid", offline_uuid(&settings.player_name)),
        ("auth_access_token", "0".into()),
        (
            "auth_session",
            format!("token:0:{}", offline_uuid(&settings.player_name)),
        ),
        ("clientid", String::new()),
        ("auth_xuid", String::new()),
        ("user_type", "legacy".into()),
        ("version_type", version.kind.clone()),
        ("user_properties", "{}".into()),
        ("profile_properties", "{}".into()),
        ("natives_directory", natives.to_string_lossy().into_owned()),
        ("launcher_name", "NSCL".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
        ("classpath", classpath.clone()),
        (
            "classpath_separator",
            if cfg!(windows) { ";" } else { ":" }.into(),
        ),
        (
            "library_directory",
            root.join("libraries").to_string_lossy().into_owned(),
        ),
        ("resolution_width", "1280".into()),
        ("resolution_height", "720".into()),
    ]);
    let (mut jvm, mut game) = if let Some(arguments) = &version.arguments {
        (
            resolve_arguments(&arguments.jvm, platform, settings.demo_mode, &values)?,
            resolve_arguments(&arguments.game, platform, settings.demo_mode, &values)?,
        )
    } else if let Some(arguments) = &version.legacy_arguments {
        let mut jvm = vec![
            format!("-Djava.library.path={}", natives.display()),
            "-cp".into(),
            classpath.clone(),
        ];
        if platform.os == "osx" {
            jvm.push("-XstartOnFirstThread".into());
        }
        (
            jvm,
            arguments
                .split_whitespace()
                .map(|arg| substitute(arg, &values))
                .collect::<Result<Vec<_>>>()?,
        )
    } else {
        return Err("版本未提供启动参数，不支持该历史客户端。".into());
    };
    if !jvm
        .iter()
        .any(|arg| matches!(arg.as_str(), "-cp" | "-classpath" | "--class-path"))
    {
        jvm.extend(["-cp".to_owned(), classpath]);
    }
    if let Some(logging) = &version.logging {
        let path =
            super::download::safe_join(&assets.join("log_configs"), &logging.client.file.id)?;
        values.insert("path", path.to_string_lossy().into_owned());
        jvm.push(substitute(&logging.client.argument, &values)?);
    }
    jvm.push(format!("-Xmx{}M", settings.memory));
    for property in [
        "java.io.tmpdir",
        "jna.tmpdir",
        "org.lwjgl.system.SharedLibraryExtractPath",
        "io.netty.native.workdir",
    ] {
        jvm.push(format!("-D{property}={}", natives.display()));
    }
    jvm.push(version.main_class.clone());
    if settings.demo_mode && !game.iter().any(|argument| argument == "--demo") {
        game.push("--demo".into());
    }
    jvm.extend(game);
    Ok(jvm)
}

fn resolve_arguments(
    arguments: &[Argument],
    platform: &Platform,
    demo: bool,
    values: &BTreeMap<&str, String>,
) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for argument in arguments {
        match argument {
            Argument::Plain(value) => result.push(substitute(value, values)?),
            Argument::Conditional { rules, value } if rules_allow(rules, platform, demo) => {
                match value {
                    ArgumentValue::One(value) => result.push(substitute(value, values)?),
                    ArgumentValue::Many(values_list) => {
                        for value in values_list {
                            result.push(substitute(value, values)?);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(result)
}

fn substitute(value: &str, values: &BTreeMap<&str, String>) -> Result<String> {
    let pattern = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
    for capture in pattern.captures_iter(value) {
        if !values.contains_key(&capture[1]) {
            return Err(format!("不支持的启动参数变量：{}", &capture[1]));
        }
    }
    Ok(pattern
        .replace_all(value, |captures: &regex::Captures<'_>| {
            values[&captures[1]].as_str()
        })
        .into_owned())
}

pub fn java_major(output: &str) -> Option<u32> {
    let regex = regex::Regex::new(
        r#"(?:java\.version\s*=\s*|(?:openjdk|java) version ")(\d+)(?:\.(\d+))?"#,
    )
    .unwrap();
    let capture = regex.captures(output)?;
    let major = capture[1].parse::<u32>().ok()?;
    if major == 1 {
        capture.get(2)?.as_str().parse().ok()
    } else {
        Some(major)
    }
}

pub fn find_java(configured: &str, required: u32) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    let explicit = !configured.trim().is_empty();
    if explicit {
        candidates.push(PathBuf::from(configured.trim().trim_matches('"')));
    } else {
        if let Some(home) = std::env::var_os("JAVA_HOME") {
            candidates.push(PathBuf::from(home).join("bin").join(java_executable()));
        }
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                candidates.push(dir.join(java_executable()));
            }
        }
        let roots = if cfg!(windows) {
            let base = PathBuf::from(
                std::env::var_os("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into()),
            );
            vec![
                base.join("Java"),
                base.join("Eclipse Adoptium"),
                base.join("Microsoft"),
                base.join("Zulu"),
            ]
        } else if cfg!(target_os = "macos") {
            vec![PathBuf::from("/Library/Java/JavaVirtualMachines")]
        } else {
            vec![PathBuf::from("/usr/lib/jvm")]
        };
        for root in roots {
            if let Ok(entries) = fs::read_dir(root) {
                for entry in entries.flatten() {
                    candidates.push(
                        entry
                            .path()
                            .join(if cfg!(target_os = "macos") {
                                "Contents/Home/bin"
                            } else {
                                "bin"
                            })
                            .join(java_executable()),
                    );
                }
            }
        }
    }
    let mut errors = Vec::new();
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        let probe = if candidate
            .file_name()
            .is_some_and(|name| name == "javaw.exe")
        {
            candidate.with_file_name("java.exe")
        } else {
            candidate.clone()
        };
        match hidden_command(&probe)
            .args(["-XshowSettings:properties", "-version"])
            .output()
        {
            Ok(output) => {
                let text = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                let wrong_bits = usize::BITS == 64 && text.contains("sun.arch.data.model = 32");
                if output.status.success() && java_major(&text) == Some(required) && !wrong_bits {
                    return Ok(candidate);
                }
                errors.push(format!(
                    "{}: Java {:?}",
                    candidate.display(),
                    java_major(&text)
                ));
            }
            Err(error) => errors.push(format!("{}: {error}", candidate.display())),
        }
    }
    Err(format!(
        "未找到匹配的 Java {required}（{}位）。请安装对应版本并在设置中选择 java 可执行文件。{}",
        usize::BITS,
        if explicit {
            format!("\n{}", errors.join("\n"))
        } else {
            String::new()
        }
    ))
}

fn java_executable() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}
