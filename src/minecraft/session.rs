use super::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum LogKind {
    Launch,
    Game,
    Debug,
    Crash,
    Jvm,
}
impl LogKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Launch => "启动日志",
            Self::Game => "本次游戏日志",
            Self::Debug => "加载器调试日志",
            Self::Crash => "本次崩溃报告",
            Self::Jvm => "JVM 错误日志",
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct SessionLog {
    pub kind: LogKind,
    pub text: String,
}
#[derive(Serialize, Deserialize)]
struct Stamp {
    size: u64,
    modified: Option<SystemTime>,
    tail: String,
}
#[derive(Serialize, Deserialize)]
pub struct LaunchSession {
    root: PathBuf,
    id: String,
    started: SystemTime,
    before: BTreeMap<PathBuf, Stamp>,
}

fn checked(path: &Path) -> Result<()> {
    for p in path.ancestors() {
        crate::instances::reject_link(p)?;
    }
    Ok(())
}
pub fn tail(path: &Path, offset: u64) -> std::io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    file.seek(SeekFrom::Start(
        offset.max(size.saturating_sub(65536)).min(size),
    ))?;
    let mut bytes = Vec::new();
    file.take(65536).read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn stamp(path: &Path) -> Result<Stamp> {
    checked(path)?;
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(Stamp {
        size: meta.len(),
        modified: meta.modified().ok(),
        tail: format!(
            "{:x}",
            Sha256::digest(tail(path, 0).map_err(|e| e.to_string())?)
        ),
    })
}
fn candidates(root: &Path) -> Vec<(PathBuf, LogKind)> {
    let mut result = vec![
        (PathBuf::from("logs/latest.log"), LogKind::Game),
        (PathBuf::from("logs/debug.log"), LogKind::Debug),
    ];
    for (folder, kind) in [("crash-reports", LogKind::Crash), ("", LogKind::Jvm)] {
        let directory = root.join(folder);
        if checked(&directory).is_err() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(directory) {
            for e in entries.take(1000).flatten() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                if (folder.is_empty() && name.starts_with("hs_err_pid") && name.ends_with(".log"))
                    || (!folder.is_empty() && name.starts_with("crash-") && name.ends_with(".txt"))
                {
                    result.push((PathBuf::from(folder).join(name.as_ref()), kind));
                }
            }
        }
    }
    result
}

impl LaunchSession {
    pub fn begin(root: &Path) -> Result<Self> {
        let started = SystemTime::now();
        checked(root)?;
        let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
        let before = candidates(&root)
            .into_iter()
            .filter_map(|(p, _)| stamp(&root.join(&p)).ok().map(|s| (p, s)))
            .collect();
        let s = Self {
            root,
            id: format!("session-{}", uuid::Uuid::new_v4().simple()),
            started,
            before,
        };
        checked(&s.directory())?;
        fs::create_dir_all(s.directory()).map_err(|e| e.to_string())?;
        crate::store::write_json(&s.directory().join("session.json"), &s)?;
        Ok(s)
    }
    fn directory(&self) -> PathBuf {
        self.root.join("logs/nscl-sessions").join(&self.id)
    }
    pub fn log_path(&self) -> PathBuf {
        self.directory().join("launch.log")
    }
    fn collect(&self) -> Vec<SessionLog> {
        let mut result = Vec::new();
        let mut files = candidates(&self.root);
        files.sort_by_key(|(p, _)| {
            std::cmp::Reverse(
                fs::metadata(self.root.join(p))
                    .and_then(|m| m.modified())
                    .ok(),
            )
        });
        for (p, kind) in files {
            let path = self.root.join(&p);
            if let Some(old) = self.before.get(&p)
                && let Ok(meta) = fs::metadata(&path)
                && old.size == meta.len()
                && old.modified == meta.modified().ok()
            {
                continue;
            }
            let Ok(now) = stamp(&path) else {
                continue;
            };
            if !self.before.contains_key(&p) && now.modified.is_none_or(|t| t < self.started) {
                continue;
            }
            let mut offset = 0;
            if let Some(old) = self.before.get(&p) {
                if now.size == old.size && now.tail == old.tail && now.modified == old.modified {
                    continue;
                }
                if now.size > old.size {
                    // Preserve the pre-launch suffix as a prefix check for append-only logs.
                    let prefix_matches = (|| -> std::io::Result<bool> {
                        let mut f = fs::File::open(&path)?;
                        f.seek(SeekFrom::Start(old.size.saturating_sub(65536)))?;
                        let mut b = Vec::new();
                        f.take(old.size.min(65536)).read_to_end(&mut b)?;
                        Ok(format!("{:x}", Sha256::digest(b)) == old.tail)
                    })()
                    .unwrap_or(false);
                    if prefix_matches {
                        offset = old.size;
                    }
                }
            }
            if let Ok(bytes) = tail(&path, offset) {
                result.push(SessionLog {
                    kind,
                    text: String::from_utf8_lossy(&bytes).into_owned(),
                });
            }
            if result.len() >= 10 {
                break;
            }
        }
        result
    }
    pub fn finish(&self) -> Result<()> {
        crate::store::write_json(&self.directory().join("collected.json"), &self.collect())
    }
    pub fn logs(path: &Path) -> Result<Option<Vec<SessionLog>>> {
        let Some(parent) = path.parent() else {
            return Ok(None);
        };
        let manifest = parent.join("session.json");
        if !manifest.exists() {
            return Ok(None);
        }
        checked(&manifest)?;
        let file = fs::File::open(&manifest).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024 {
            return Err("启动会话信息过大。".into());
        }
        let s: Self = serde_json::from_reader(file).map_err(|e| e.to_string())?;
        super::validate_id(&s.id)?;
        if s.log_path() != path {
            return Err("启动日志与会话不匹配。".into());
        }
        let saved = parent.join("collected.json");
        let mut logs = if saved.exists() {
            checked(&saved)?;
            let file = fs::File::open(saved).map_err(|e| e.to_string())?;
            if file.metadata().map_err(|e| e.to_string())?.len() > 2 * 1024 * 1024 {
                return Err("会话日志归档过大。".into());
            }
            serde_json::from_reader(file).map_err(|e| e.to_string())?
        } else {
            s.collect()
        };
        logs.insert(
            0,
            SessionLog {
                kind: LogKind::Launch,
                text: String::from_utf8_lossy(&tail(path, 0).map_err(|e| e.to_string())?)
                    .into_owned(),
            },
        );
        Ok(Some(logs))
    }
}

#[cfg(test)]
mod tests;
