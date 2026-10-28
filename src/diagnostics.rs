use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

pub struct Log {
    pub name: &'static str,
    pub text: String,
}

pub struct Finding {
    pub source: &'static str,
    pub line: String,
    pub reason: &'static str,
    pub advice: &'static str,
    signature: &'static str,
}

pub struct Report {
    pub logs: Vec<Log>,
    pub findings: Vec<Finding>,
}

struct Rule {
    signature: &'static str,
    reason: &'static str,
    advice: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        signature: "outofmemoryerror",
        reason: "Java 内存不足",
        advice: "关闭其他占用内存的程序，在实例或启动器设置中适当增加内存；不要超过机器可用内存。",
    },
    Rule {
        signature: "could not reserve enough space",
        reason: "Java 无法预留所需内存",
        advice: "降低分配的内存，检查 Java 是否为 64 位并确认系统可用内存。",
    },
    Rule {
        signature: "unsupportedclassversionerror",
        reason: "Java 版本可能过低",
        advice: "为该游戏版本选择兼容的 Java，建议启用自动选择 Java 后重试。",
    },
    Rule {
        signature: "incompatible mods found",
        reason: "模组或加载器不兼容",
        advice: "检查日志中列出的模组、依赖及游戏和加载器版本，更新或移除不兼容项。",
    },
    Rule {
        signature: "mod resolution encountered an incompatible mod set",
        reason: "模组依赖不兼容",
        advice: "检查模组所需的依赖及版本，安装缺失依赖或调整模组版本。",
    },
    Rule {
        signature: "noclassdeffounderror",
        reason: "类或模组依赖缺失，也可能由其他错误引起",
        advice: "先查看附近的首个 Caused by，检查缺失的模组依赖与游戏版本，再尝试修复实例。",
    },
    Rule {
        signature: "glfw error",
        reason: "图形初始化可能失败",
        advice: "更新显卡驱动，检查游戏需要的 OpenGL 支持；虚拟机中检查 3D 加速。",
    },
    Rule {
        signature: "accessdeniedexception",
        reason: "文件访问被拒绝",
        advice: "检查游戏目录权限、文件占用和安全软件，并确认目录可写。",
    },
    Rule {
        signature: "connection timed out",
        reason: "网络连接超时",
        advice: "检查网络、代理和目标服务可用性，然后重试。",
    },
];

fn tail(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(65536)))?;
    let mut bytes = Vec::new();
    file.take(65536).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

impl Report {
    pub fn load(launch: &Path) -> std::io::Result<Self> {
        let mut logs = vec![Log {
            name: "启动日志",
            text: tail(launch)?,
        }];
        if let Some(parent) = launch.parent()
            && let Ok(text) = tail(&parent.join("latest.log"))
        {
            logs.push(Log {
                name: "游戏日志 latest.log（可能来自上次运行）",
                text,
            });
        }
        let mut findings = Vec::new();
        for log in &logs {
            for rule in RULES {
                if let Some(line) = log
                    .text
                    .lines()
                    .rev()
                    .find(|line| line.to_ascii_lowercase().contains(rule.signature))
                {
                    findings.push(Finding {
                        source: log.name,
                        line: line.chars().take(600).collect(),
                        reason: rule.reason,
                        advice: rule.advice,
                        signature: rule.signature,
                    });
                }
            }
        }
        Ok(Self { logs, findings })
    }

    pub fn export(&self, path: &Path) -> Result<(), String> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("无法创建诊断包（请选择不存在的文件）：{e}"))?;
        let result = (|| -> Result<(), String> {
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file("report.txt", zip::write::SimpleFileOptions::default())
                .map_err(|e| e.to_string())?;
            writeln!(zip, "NSCL 脱敏诊断包\n仅包含固定错误特征和建议，不包含原始日志或用户信息。\n日志仅分析末尾 64 KiB；游戏日志可能来自上次运行。\n诊断仅供参考。\n")
                .map_err(|e| e.to_string())?;
            if self.findings.is_empty() {
                writeln!(zip, "未识别出常见错误特征。请在本地查看原始日志。")
                    .map_err(|e| e.to_string())?;
            }
            for finding in &self.findings {
                writeln!(
                    zip,
                    "来源：{}\n错误特征：{}\n可能原因：{}\n处理建议：{}\n",
                    finding.source, finding.signature, finding.reason, finding.advice
                )
                .map_err(|e| e.to_string())?;
            }
            zip.finish().map_err(|e| e.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(path);
        }
        result.map_err(|e| format!("导出诊断包失败：{e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> tempfile::TempDir {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
        fs::create_dir_all(&parent).unwrap();
        tempfile::tempdir_in(parent).unwrap()
    }
    #[test]
    fn diagnosis_uses_log_evidence_and_export_never_contains_original_secrets() {
        let root = fixture();
        let launch = root.path().join("nscl-launch.log");
        fs::write(
            &launch,
            "C:\\Users\\PrivateName token=secret123 java.lang.OutOfMemoryError: secret123\n",
        )
        .unwrap();
        fs::write(
            root.path().join("latest.log"),
            "user@example.com UnsupportedClassVersionError session=secret456\n",
        )
        .unwrap();
        let report = Report::load(&launch).unwrap();
        assert_eq!(report.findings.len(), 2);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.line.contains("OutOfMemoryError") && f.source == "启动日志")
        );
        let path = root.path().join("diagnostics.zip");
        report.export(&path).unwrap();
        let mut archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let mut exported = String::new();
        archive
            .by_name("report.txt")
            .unwrap()
            .read_to_string(&mut exported)
            .unwrap();
        assert!(exported.contains("outofmemoryerror"));
        assert!(exported.contains("unsupportedclassversionerror"));
        for secret in ["PrivateName", "secret123", "secret456", "user@example.com"] {
            assert!(!exported.contains(secret));
        }
        drop(archive);
        assert!(report.export(&path).is_err());
        assert!(zip::ZipArchive::new(fs::File::open(&path).unwrap()).is_ok());
    }

    #[test]
    fn no_known_error_is_reported_as_unknown() {
        let root = fixture();
        let launch = root.path().join("nscl-launch.log");
        fs::write(&launch, "普通输出\n").unwrap();
        let report = Report::load(&launch).unwrap();
        assert!(report.findings.is_empty());
        let path = root.path().join("report.zip");
        report.export(&path).unwrap();
        let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        let mut text = String::new();
        archive
            .by_name("report.txt")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert!(text.contains("未识别"));
    }
}
