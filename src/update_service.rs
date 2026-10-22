use nscl::updater::{self, ReadyUpdate, Release};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};

enum Event {
    Checked(Result<Option<Release>, String>),
    Progress(u64, u64),
    Downloaded(Result<ReadyUpdate, String>),
}
pub struct UpdateService {
    pub started: bool,
    pub busy: bool,
    pub downloading: bool,
    pub status: String,
    pub release: Option<Release>,
    pub ready: Option<ReadyUpdate>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}
impl UpdateService {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let error = std::fs::read_to_string(updater::cache_directory().join("last-error.txt")).ok();
        let status = error
            .as_ref()
            .map(|e| format!("上次更新未完成：{e}"))
            .unwrap_or_else(|| "从 GitHub Releases 获取正式版本。".into());
        Self {
            started: error.is_some(),
            busy: false,
            downloading: false,
            status,
            release: None,
            ready: None,
            sender,
            receiver,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn available(&self) -> bool {
        self.release
            .as_ref()
            .is_some_and(|r| r.newer_than(env!("CARGO_PKG_VERSION")).unwrap_or(false))
    }
    pub fn check(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.started = true;
        self.busy = true;
        self.ready = None;
        self.status = "正在检查 GitHub Releases……".into();
        let (sender, ctx) = (self.sender.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = sender.send(Event::Checked(updater::check()));
            ctx.request_repaint();
        });
    }
    pub fn download(&mut self, ctx: &egui::Context) {
        if self.busy || !self.available() {
            return;
        }
        let Some(release) = self.release.clone() else {
            return;
        };
        self.busy = true;
        self.downloading = true;
        self.ready = None;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.status = "正在下载并校验更新……".into();
        let (sender, ctx, cancel) = (self.sender.clone(), ctx.clone(), self.cancel.clone());
        std::thread::spawn(move || {
            let result = updater::download(&release, &cancel, |done, total| {
                let _ = sender.send(Event::Progress(done, total));
                ctx.request_repaint();
            });
            let _ = sender.send(Event::Downloaded(result));
            ctx.request_repaint();
        });
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn poll(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Checked(result) => {
                    self.busy = false;
                    match result {
                        Ok(Some(release)) => {
                            self.status = if release
                                .newer_than(env!("CARGO_PKG_VERSION"))
                                .unwrap_or(false)
                            {
                                format!("发现新版本 {}", release.tag_name)
                            } else {
                                "当前已是最新正式版本。".into()
                            };
                            self.release = Some(release);
                        }
                        Ok(None) => {
                            self.release = None;
                            self.status = "GitHub 暂无已发布的正式 Release。".into();
                        }
                        Err(error) => self.status = error,
                    }
                }
                Event::Progress(done, total) => {
                    self.status = format!(
                        "下载更新 {:.1} / {:.1} MiB",
                        done as f64 / 1048576.,
                        total as f64 / 1048576.
                    )
                }
                Event::Downloaded(result) => {
                    self.busy = false;
                    self.downloading = false;
                    match result {
                        Ok(path) => {
                            self.ready = Some(path);
                            self.status = "更新已校验，退出游戏后可重启并更新。".into();
                        }
                        Err(error) => self.status = error,
                    }
                }
            }
        }
    }
}
impl Drop for UpdateService {
    fn drop(&mut self) {
        self.cancel();
    }
}
