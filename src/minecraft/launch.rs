use super::{
    Result,
    download::{safe_join, validate_id, verified},
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
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
};

#[cfg(windows)]
mod windows;

pub struct LaunchPlan {
    pub java: PathBuf,
    pub arguments: Vec<String>,
    pub game_directory: PathBuf,
    pub log_path: PathBuf,
}

impl std::fmt::Debug for LaunchPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchPlan")
            .field("java", &self.java)
            .field("game_directory", &self.game_directory)
            .field("log_path", &self.log_path)
            .field("arguments", &"<redacted>")
            .finish()
    }
}

#[derive(Clone)]
pub struct LaunchIdentity {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub xuid: String,
    pub client_id: String,
    pub online: bool,
}
impl LaunchIdentity {
    pub fn offline(name: &str) -> Self {
        Self {
            name: name.into(),
            uuid: offline_uuid(name),
            access_token: "0".into(),
            xuid: String::new(),
            client_id: String::new(),
            online: false,
        }
    }
    pub fn microsoft(session: &crate::accounts::Session) -> Self {
        Self {
            name: session.account.name.clone(),
            uuid: session.account.id.clone(),
            access_token: session.access_token.clone(),
            xuid: session.account.xuid.clone(),
            client_id: session.account.client_id.clone(),
            online: true,
        }
    }
}

pub fn prepare_launch(root: &Path, id: &str, settings: &Settings) -> Result<LaunchPlan> {
    prepare_instance_launch(
        root,
        &crate::instances::Instance {
            id: id.into(),
            name: id.into(),
            version_id: id.into(),
            ..Default::default()
        },
        settings,
        &LaunchIdentity::offline(&settings.player_name),
    )
}

pub fn prepare_instance_launch(
    root: &Path,
    instance: &crate::instances::Instance,
    settings: &Settings,
    identity: &LaunchIdentity,
) -> Result<LaunchPlan> {
    instance.validate()?;
    let mut settings = instance.effective_settings(settings);
    let id = instance.version_id.as_str();
    super::validate_local_id(id)?;
    validate_player_name(&identity.name)?;
    if !crate::accounts::valid_profile_id(&identity.uuid)
        || (identity.online && (identity.access_token.is_empty() || identity.access_token == "0"))
    {
        return Err("启动身份无效，请重新登录。".into());
    }
    if !settings.auto_memory && !(1024..=32768).contains(&settings.memory) {
        return Err("游戏内存必须为 1024–32768 MiB。".into());
    }
    let root = absolute_root(root)?;
    let version_dir = root.join("versions").join(id);
    let platform = Platform::current();
    let marker = version_dir.join("nscl-instance.json");
    let (installed, version, client) = if marker.exists() {
        let installed: InstalledVersion = read_json(&marker)?;
        if installed.id != id || installed.platform != platform.key() {
            return Err("该实例未针对当前系统完成安装，请在下载页重新安装。".into());
        }
        let metadata = version_dir.join(format!("{id}.json"));
        if !verified(&metadata, &installed.metadata_sha1, None)? {
            return Err("版本元数据损坏，请重新安装/修复。".into());
        }
        (
            installed,
            read_json::<Version>(&metadata)?,
            version_dir.join(format!("{id}.jar")),
        )
    } else {
        let installed = super::external::describe(&root, id)?;
        let (version, client) = super::external::prepare(&root, id, &platform)?;
        (installed, version, client)
    };
    if version.id != id || version.inherits_from.is_some() {
        return Err("该版本尚未完成安装或版本合并，请重新安装。".into());
    }
    for artifact in &installed.extra_files {
        let path = safe_join(
            &root.join("libraries"),
            artifact.path.as_deref().ok_or("安装记录缺少库文件路径。")?,
        )?;
        if !verified(&path, &artifact.sha1, artifact.size)? {
            return Err(format!(
                "加载器生成文件缺失或损坏：{}。请重新安装/修复加载器。",
                path.display()
            ));
        }
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
    let mut files = plan(&root, &version, &index, &platform)?;
    let default_client = version_dir.join(format!("{id}.jar"));
    for job in &mut files.jobs {
        if job.path == default_client {
            job.path = client.clone();
        }
    }
    for path in &mut files.classpath {
        if *path == default_client {
            *path = client.clone();
        }
    }
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
    let java = crate::java::find(settings.configured_java(), version.java_major())?;
    if settings.auto_memory {
        settings.memory =
            crate::memory::automatic(version.java_major(), installed.loader.is_some())?;
    }
    let game_directory = instance.game_directory(&root)?;
    fs::create_dir_all(game_directory.join("logs")).map_err(|e| e.to_string())?;
    let log_path = game_directory.join("logs/nscl-launch.log");
    let arguments = launch_arguments_for(
        &root,
        &version,
        &index,
        &files.classpath,
        &natives,
        &platform,
        LaunchContext {
            settings: &settings,
            identity,
            game_directory: &game_directory,
        },
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
    // Prefer the runtime's Windows GUI launcher for games, and retain the console
    // executable for installer tools and runtimes without javaw.
    #[cfg(windows)]
    let mut command = {
        let javaw = plan.java.with_file_name("javaw.exe");
        if javaw.is_file() {
            std::process::Command::new(javaw)
        } else {
            hidden_command(&plan.java)
        }
    };
    #[cfg(not(windows))]
    let mut command = hidden_command(&plan.java);
    let mut child = command
        .args(&plan.arguments)
        .current_dir(&plan.game_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|e| format!("启动 Java 失败：{e}"))?;
    on_started(child.id(), &plan.log_path);
    let status = child.wait().map_err(|e| e.to_string())?;
    let code = status.code().unwrap_or(-1);
    if code != 0
        && let Some(error) = fabric_remap_failure(&plan.log_path)
    {
        return Err(format!(
            "{error}\n退出码：{code}；启动日志：{}",
            plan.log_path.display()
        ));
    }
    Ok(code)
}

fn fabric_remap_failure(log_path: &Path) -> Option<String> {
    // Remapping happens before Minecraft starts. Bound the diagnostic read and
    // tolerate non-UTF-8 output without changing the original log on disk.
    let mut bytes = Vec::new();
    File::open(log_path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut bytes)
        .ok()?;
    let log = String::from_utf8_lossy(&bytes);
    if !is_fabric_remap_write_failure(&log) {
        return None;
    }
    #[cfg(windows)]
    if windows::is_low_integrity().unwrap_or(false) {
        return Some(
            "Fabric 首次映射失败：启动器正处于 Windows 低完整性权限环境，Java 可能把可写的缓存误判为只读。请退出 NSCL，将启动器 EXE 复制到普通用户目录（沙箱或受限开发目录之外）后重新打开；无需以管理员身份运行，也无需重装游戏。"
                .into(),
        );
    }
    Some(
        "Fabric 首次映射失败：无法写入映射 JAR。请检查实例目录的写入权限、磁盘空间及安全软件拦截情况；无需删除存档。"
            .into(),
    )
}

fn is_fabric_remap_write_failure(log: &str) -> bool {
    log.contains("error remapping game jars")
        && log.contains("OutputConsumerPath")
        && log.contains("can't be written")
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

#[cfg(test)]
pub fn launch_arguments(
    root: &Path,
    version: &Version,
    index: &AssetIndex,
    classpath: &[PathBuf],
    natives: &Path,
    platform: &Platform,
    settings: &Settings,
) -> Result<Vec<String>> {
    launch_arguments_for(
        root,
        version,
        index,
        classpath,
        natives,
        platform,
        LaunchContext {
            settings,
            identity: &LaunchIdentity::offline(&settings.player_name),
            game_directory: &root.join("instances").join(&version.id),
        },
    )
}

pub(super) struct LaunchContext<'a> {
    pub(super) settings: &'a Settings,
    pub(super) identity: &'a LaunchIdentity,
    pub(super) game_directory: &'a Path,
}

pub(super) fn launch_arguments_for(
    root: &Path,
    version: &Version,
    index: &AssetIndex,
    classpath: &[PathBuf],
    natives: &Path,
    platform: &Platform,
    context: LaunchContext<'_>,
) -> Result<Vec<String>> {
    let settings = context.settings;
    let identity = context.identity;
    let demo = settings.demo_mode && !identity.online;
    let classpath = std::env::join_paths(classpath)
        .map_err(|e| format!("类路径包含非法分隔符：{e}"))?
        .to_string_lossy()
        .into_owned();
    let game_dir = context.game_directory;
    let assets = root.join("assets");
    let game_assets = if index.virtual_assets {
        assets.join("virtual").join(&version.asset_index.id)
    } else {
        assets.clone()
    };
    let mut values = BTreeMap::from([
        ("auth_player_name", identity.name.clone()),
        ("version_name", version.id.clone()),
        ("game_directory", game_dir.to_string_lossy().into_owned()),
        ("assets_root", assets.to_string_lossy().into_owned()),
        ("game_assets", game_assets.to_string_lossy().into_owned()),
        ("assets_index_name", version.asset_index.id.clone()),
        ("auth_uuid", identity.uuid.clone()),
        ("auth_access_token", identity.access_token.clone()),
        (
            "auth_session",
            format!("token:{}:{}", identity.access_token, identity.uuid),
        ),
        ("clientid", identity.client_id.clone()),
        ("auth_xuid", identity.xuid.clone()),
        (
            "user_type",
            if identity.online { "msa" } else { "legacy" }.into(),
        ),
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
            resolve_arguments(&arguments.jvm, platform, demo, &values)?,
            resolve_arguments(&arguments.game, platform, demo, &values)?,
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
    if demo && !game.iter().any(|argument| argument == "--demo") {
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

#[cfg(test)]
pub use crate::java::java_major;
