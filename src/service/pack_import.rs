use super::{GameService, Task};
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "pack_import_tests.rs"]
mod tests;

pub enum PackImportStatus {
    Running,
    Succeeded(String),
    Failed(String),
    Cancelled,
}

pub struct PackImport {
    pub name: String,
    pub status: PackImportStatus,
    pub cancelling: bool,
    started: Instant,
    finished: Option<Instant>,
}

impl PackImport {
    pub fn elapsed(&self) -> Duration {
        self.finished.unwrap_or_else(Instant::now) - self.started
    }
}

impl GameService {
    pub(super) fn begin_pack_import(&mut self, name: String) {
        self.begin(Task::Content);
        self.pack_import = Some(PackImport {
            name,
            status: PackImportStatus::Running,
            cancelling: false,
            started: Instant::now(),
            finished: None,
        });
        self.progress.stage = "正在读取整合包".into();
    }

    pub(super) fn finish_pack_import(&mut self) {
        if self.task != Task::Idle {
            return;
        }
        if let Some(import) = &mut self.pack_import
            && matches!(import.status, PackImportStatus::Running)
        {
            import.status = if let Some(error) = &self.error {
                PackImportStatus::Failed(error.clone())
            } else if self.notice == "操作已取消。" {
                PackImportStatus::Cancelled
            } else {
                PackImportStatus::Succeeded(self.notice.clone())
            };
            import.finished = Some(Instant::now());
        }
    }

    pub fn cancel_pack_import(&mut self) {
        self.cancel();
        if let Some(import) = &mut self.pack_import {
            import.cancelling = true;
        }
    }

    pub fn dismiss_pack_import(&mut self) {
        if self
            .pack_import
            .as_ref()
            .is_some_and(|import| matches!(import.status, PackImportStatus::Running))
        {
            return;
        }
        if let Some(import) = self.pack_import.take()
            && let PackImportStatus::Failed(error) = import.status
            && self.error.as_ref() == Some(&error)
        {
            self.error = None;
        }
    }
}
