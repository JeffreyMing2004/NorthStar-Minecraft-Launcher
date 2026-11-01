use crate::minecraft::{LoaderVersion, VersionEntry};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum State {
    Queued,
    Running,
    Complete,
    Failed,
    Cancelled,
}
impl State {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Queued => "等待中",
            Self::Running => "进行中",
            Self::Complete => "已完成",
            Self::Failed => "失败",
            Self::Cancelled => "已取消",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub entry: VersionEntry,
    pub loader: Option<LoaderVersion>,
    pub state: State,
    pub created: u64,
    pub message: String,
}
pub struct Queue {
    pub items: Vec<Item>,
    pub active: Option<String>,
    pub enabled: bool,
    pub error: Option<String>,
    path: PathBuf,
}
impl Queue {
    pub fn load(root: &Path) -> Self {
        let path = root.join(".nscl/download-tasks.json");
        let result = (|| -> Result<Vec<Item>, String> {
            for p in path.ancestors() {
                crate::instances::reject_link(p)?;
            }
            if !path.exists() {
                return Ok(Vec::new());
            }
            let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            if file.metadata().map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024 {
                return Err("下载任务记录过大。".into());
            }
            let mut items: Vec<Item> =
                serde_json::from_reader(file).map_err(|_| "下载任务记录损坏，原文件已保留。")?;
            if items.len() > 2000 {
                return Err("下载任务记录过多。".into());
            }
            for item in &mut items {
                if item.state == State::Running {
                    item.state = State::Queued;
                    item.message = "上次退出前未完成，请继续队列。".into();
                }
            }
            Ok(items)
        })();
        let (items, error) = match result {
            Ok(items) => (items, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        Self {
            items,
            active: None,
            enabled: false,
            error,
            path,
        }
    }
    pub fn root_matches(&self, root: &Path) -> bool {
        self.path == root.join(".nscl/download-tasks.json")
    }
    pub fn save(&mut self) {
        let result = (|| {
            for p in self.path.ancestors() {
                crate::instances::reject_link(p)?;
            }
            crate::store::write_json(&self.path, &self.items)
        })();
        if let Err(e) = result {
            self.error = Some(e);
            self.enabled = false;
        }
    }
    pub fn enqueue(
        &mut self,
        entry: VersionEntry,
        loader: Option<LoaderVersion>,
        running: bool,
    ) -> Option<String> {
        if self.error.is_some() {
            return None;
        }
        let id = format!("task-{}", uuid::Uuid::new_v4().simple());
        self.items.push(Item {
            id: id.clone(),
            entry,
            loader,
            state: if running {
                State::Running
            } else {
                State::Queued
            },
            created: crate::accounts::now(),
            message: String::new(),
        });
        if running {
            self.active = Some(id.clone());
        }
        // Keep pending work; trim only completed history.
        while self.items.len() > 500 {
            if let Some(i) = self
                .items
                .iter()
                .position(|i| matches!(i.state, State::Complete | State::Failed | State::Cancelled))
            {
                self.items.remove(i);
            } else {
                break;
            }
        }
        self.save();
        Some(id)
    }
    pub fn finish(&mut self, state: State, message: String) {
        if let Some(id) = self.active.take() {
            if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
                item.state = state;
                item.message = message;
            }
            self.save();
        }
    }
}
pub struct Rate {
    start: Instant,
    last: Instant,
    bytes: u64,
    pub speed: f64,
}
impl Default for Rate {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            last: Instant::now(),
            bytes: 0,
            speed: 0.0,
        }
    }
}
impl Rate {
    pub fn update(&mut self, bytes: u64) {
        let dt = self.last.elapsed().as_secs_f64();
        if bytes >= self.bytes && dt > 0.1 {
            let sample = (bytes - self.bytes) as f64 / dt;
            self.speed = if self.speed == 0.0 {
                sample
            } else {
                self.speed * 0.7 + sample * 0.3
            };
            self.last = Instant::now();
            self.bytes = bytes;
        } else if bytes < self.bytes {
            self.bytes = bytes;
            self.last = Instant::now();
        }
    }
    pub fn label(&self, p: &crate::minecraft::Progress) -> String {
        let eta = p
            .total_bytes
            .filter(|_| self.speed > 1.0)
            .map(|n| {
                format!(
                    " · 预计剩余 {:.0} 秒",
                    n.saturating_sub(p.bytes_downloaded) as f64 / self.speed
                )
            })
            .unwrap_or_default();
        format!(
            "{:.1} MiB/s · 已用 {} 秒{eta}",
            self.speed / 1048576.0,
            self.start.elapsed().as_secs()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_survives_restart_without_automatically_executing() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
        std::fs::create_dir_all(&parent).unwrap();
        let root = tempfile::tempdir_in(parent).unwrap();
        let mut q = Queue::load(root.path());
        q.enqueue(
            VersionEntry {
                id: "test".into(),
                kind: "release".into(),
                url: "https://example.com".into(),
                sha1: "0".repeat(40),
                release_time: String::new(),
            },
            None,
            true,
        );
        let q = Queue::load(root.path());
        assert_eq!(q.items.len(), 1);
        assert!(q.items[0].state == State::Queued);
        assert!(!q.enabled);
    }
}
