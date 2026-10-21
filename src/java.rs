use crate::minecraft::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JavaRuntime {
    pub path: PathBuf,
    pub major: u32,
    pub version: String,
    pub architecture: String,
    pub vendor: String,
}
impl JavaRuntime {
    pub fn compatible(&self, required: u32) -> bool {
        self.major == required && self.architecture == std::env::consts::ARCH
    }
    pub fn label(&self) -> String {
        format!(
            "Java {} · {} · {}",
            self.version, self.architecture, self.vendor
        )
    }
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

fn property<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.trim().split_once('=')?;
        (name.trim() == key).then(|| value.trim())
    })
}

pub fn parse_runtime(path: PathBuf, output: &str) -> Result<JavaRuntime> {
    let major = java_major(output).ok_or("无法识别 Java 版本。")?;
    let architecture = match property(output, "os.arch").unwrap_or("") {
        "amd64" | "x86_64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        "x86" | "i386" | "i486" | "i586" | "i686" => "x86",
        other => other,
    }
    .to_owned();
    Ok(JavaRuntime {
        path,
        major,
        architecture,
        version: property(output, "java.version")
            .map(str::to_owned)
            .unwrap_or_else(|| major.to_string()),
        vendor: property(output, "java.vendor")
            .unwrap_or("未知发行版")
            .to_owned(),
    })
}

pub fn probe(path: &Path) -> Result<JavaRuntime> {
    let path = if path.is_dir() {
        path.join("bin").join(executable())
    } else {
        path.to_path_buf()
    };
    if !path.is_file() {
        return Err("Java 路径不存在，请选择 java/javaw 可执行文件或 Java 安装目录。".into());
    }
    let path = std::fs::canonicalize(path).map_err(|_| "无法访问 Java 路径。")?;
    let probe_path = if path
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe"))
    {
        path.with_file_name("java.exe")
    } else {
        path.clone()
    };
    let mut child = crate::minecraft::hidden_command(&probe_path)
        .args(["-XshowSettings:properties", "-version"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Java 无法执行：{e}"))?;
    let deadline = Instant::now() + Duration::from_secs(6);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(40)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Java 检测超时或执行失败，请检查可执行文件。".into());
            }
        }
    };
    let mut output = Vec::new();
    if let Some(pipe) = child.stdout.take() {
        let _ = pipe.take(65536).read_to_end(&mut output);
    }
    if let Some(pipe) = child.stderr.take() {
        let _ = pipe.take(65536).read_to_end(&mut output);
    }
    if !status.success() {
        return Err("Java 版本检测执行失败。".into());
    }
    parse_runtime(path, &String::from_utf8_lossy(&output))
}

fn executable() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        paths.push(PathBuf::from(home).join("bin").join(executable()));
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path).map(|p| p.join(executable())));
    }
    let mut roots = Vec::new();
    if cfg!(windows) {
        for base in [
            std::env::var_os("ProgramFiles"),
            std::env::var_os("ProgramFiles(x86)"),
        ]
        .into_iter()
        .flatten()
        {
            for vendor in [
                "Java",
                "Eclipse Adoptium",
                "Microsoft",
                "Zulu",
                "BellSoft",
                "Semeru",
            ] {
                roots.push(PathBuf::from(&base).join(vendor));
            }
        }
    } else if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
    } else {
        roots.push(PathBuf::from("/usr/lib/jvm"));
    }
    roots.push(
        crate::store::settings_path()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("java"),
    );
    for root in roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten().take(64) {
                paths.push(
                    entry
                        .path()
                        .join(if cfg!(target_os = "macos") {
                            "Contents/Home/bin"
                        } else {
                            "bin"
                        })
                        .join(executable()),
                );
            }
        }
    }
    let mut seen = BTreeSet::new();
    paths.retain(|p| {
        p.is_file()
            && seen.insert(
                std::fs::canonicalize(p)
                    .unwrap_or_else(|_| p.clone())
                    .to_string_lossy()
                    .to_lowercase(),
            )
    });
    paths
}

pub fn discover() -> Vec<JavaRuntime> {
    let mut runtimes: Vec<_> = candidates()
        .into_iter()
        .filter_map(|p| probe(&p).ok())
        .collect();
    runtimes.sort_by(|a, b| b.major.cmp(&a.major).then(a.path.cmp(&b.path)));
    runtimes
}

pub fn find(configured: &str, required: u32) -> Result<PathBuf> {
    if !configured.trim().is_empty() {
        let java = probe(Path::new(configured.trim().trim_matches('"')))?;
        return if java.compatible(required) {
            Ok(java.path)
        } else {
            Err(format!(
                "该实例需要 Java {required} / {}，当前选择的是 {}。",
                std::env::consts::ARCH,
                java.label()
            ))
        };
    }
    for path in candidates() {
        if let Ok(java) = probe(&path)
            && java.compatible(required)
        {
            return Ok(java.path);
        }
    }
    Err(format!(
        "未找到 Java {required} / {}。请安装匹配版本，然后在设置中扫描或指定 Java 路径。",
        std::env::consts::ARCH
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_checks_version_and_architecture() {
        let runtime = parse_runtime(
            "java".into(),
            "java.version = 17.0.12\nos.arch = amd64\njava.vendor = Eclipse Adoptium",
        )
        .unwrap();
        assert_eq!(runtime.major, 17);
        assert_eq!(runtime.architecture, "x86_64");
        assert!(!runtime.compatible(21));
        let runtime32 =
            parse_runtime("java".into(), "java.version = 1.8.0_401\nos.arch = x86").unwrap();
        assert_eq!(runtime32.major, 8);
        if cfg!(target_arch = "x86_64") {
            assert!(!runtime32.compatible(8));
        }
    }
}
