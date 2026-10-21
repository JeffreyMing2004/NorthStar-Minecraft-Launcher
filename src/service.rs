use crate::{
    minecraft::{self, Downloader, InstalledVersion, Progress, Reporter, VersionEntry},
    store::{self, Settings},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Idle,
    Manifest,
    Install,
    Launch,
}

enum Event {
    Progress(Progress),
    Versions(Vec<VersionEntry>),
    Installed(InstalledVersion),
    Failed(String),
    GameStarted(u32, PathBuf),
    GameExited(std::result::Result<i32, String>),
}

pub struct GameService {
    pub versions: Vec<VersionEntry>,
    pub installed: Vec<InstalledVersion>,
    pub selected: String,
    pub search: String,
    pub snapshots: bool,
    pub progress: Progress,
    pub error: Option<String>,
    pub notice: String,
    pub task: Task,
    pub game: Option<(u32, PathBuf)>,
    pub last_log: Option<PathBuf>,
    versions_requested: bool,
    cancel: Arc<AtomicBool>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}

impl GameService {
    pub fn new(settings: &Settings) -> Self {
        let (sender, receiver) = mpsc::channel();
        let installed = minecraft::installed_versions(&store::game_directory(settings));
        let selected = installed
            .first()
            .map(|version| version.id.clone())
            .unwrap_or_default();
        Self {
            versions: Vec::new(),
            installed,
            selected,
            search: String::new(),
            snapshots: false,
            progress: Progress::default(),
            error: None,
            notice: String::new(),
            task: Task::Idle,
            game: None,
            last_log: None,
            versions_requested: false,
            cancel: Arc::new(AtomicBool::new(false)),
            sender,
            receiver,
        }
    }

    pub fn busy(&self) -> bool {
        self.task != Task::Idle
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn refresh(&mut self, settings: &Settings) {
        self.installed = minecraft::installed_versions(&store::game_directory(settings));
        if !self.installed.iter().any(|entry| entry.id == self.selected) {
            self.selected = self
                .installed
                .first()
                .map(|entry| entry.id.clone())
                .unwrap_or_default();
        }
    }

    fn begin(&mut self, task: Task) {
        self.cancel = Arc::new(AtomicBool::new(false));
        self.task = task;
        self.error = None;
        self.notice.clear();
        self.progress = Progress::default();
    }

    pub fn poll(&mut self, settings: &Settings) {
        for _ in 0..1024 {
            let Ok(event) = self.receiver.try_recv() else {
                break;
            };
            match event {
                Event::Progress(progress) => self.progress = progress,
                Event::Versions(versions) => {
                    self.versions = versions;
                    self.task = Task::Idle;
                    self.notice.clear();
                }
                Event::Installed(version) => {
                    self.selected = version.id.clone();
                    self.refresh(settings);
                    self.task = Task::Idle;
                    self.notice = format!("Minecraft {} 安装完成，可在开始页启动。", version.id);
                }
                Event::Failed(error) => {
                    self.task = Task::Idle;
                    if error == "操作已取消。" {
                        self.notice = error;
                    } else {
                        self.error = Some(error);
                    }
                }
                Event::GameStarted(pid, path) => {
                    self.game = Some((pid, path.clone()));
                    self.last_log = Some(path);
                    self.task = Task::Idle;
                    self.notice = format!("游戏已启动（PID {pid}）");
                }
                Event::GameExited(result) => {
                    self.game = None;
                    match result {
                        Ok(0) => self.notice = "游戏已退出。".into(),
                        Ok(code) => {
                            self.error =
                                Some(format!("游戏异常退出，退出码 {code}。请查看启动日志。"))
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
            }
        }
    }

    pub fn invalidate_versions(&mut self) {
        self.versions.clear();
        self.versions_requested = false;
    }

    pub fn ensure_versions_loaded(&mut self, settings: &Settings, ctx: &egui::Context) {
        if !self.versions_requested {
            self.load_versions(settings, ctx);
        }
    }

    pub fn load_versions(&mut self, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        self.versions_requested = true;
        self.begin(Task::Manifest);
        self.versions.clear();
        let source = settings.source.clone();
        let cancel = self.cancel.clone();
        let sender = self.sender.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let reporter = reporter(&sender, &context);
            let result = Downloader::new(source, cancel, reporter)
                .and_then(|downloader| minecraft::manifest(&downloader));
            send(
                &sender,
                &context,
                match result {
                    Ok(manifest) => Event::Versions(manifest.versions),
                    Err(error) => Event::Failed(error),
                },
            );
        });
    }

    pub fn install(&mut self, entry: VersionEntry, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        if self.game.is_some() {
            self.error = Some("请先退出当前游戏，再安装或修复版本。".into());
            return;
        }
        self.begin(Task::Install);
        let root = store::game_directory(settings);
        let source = settings.source.clone();
        let cancel = self.cancel.clone();
        let sender = self.sender.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let reporter = reporter(&sender, &context);
            let result = Downloader::new(source, cancel, reporter)
                .and_then(|downloader| minecraft::install(&root, &entry, &downloader));
            send(
                &sender,
                &context,
                match result {
                    Ok(version) => Event::Installed(version),
                    Err(error) => Event::Failed(error),
                },
            );
        });
    }

    pub fn launch(&mut self, settings: &Settings, ctx: &egui::Context) {
        if self.busy() || self.game.is_some() || self.selected.is_empty() {
            return;
        }
        self.begin(Task::Launch);
        self.progress.stage = "校验游戏文件、选择 Java 并准备启动".into();
        let root = store::game_directory(settings);
        let id = self.selected.clone();
        let settings = settings.clone();
        let sender = self.sender.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let plan = match minecraft::prepare_launch(&root, &id, &settings) {
                Ok(plan) => plan,
                Err(error) => {
                    send(&sender, &context, Event::Failed(error));
                    return;
                }
            };
            let mut started = false;
            let result = minecraft::launch(&plan, |pid, path| {
                started = true;
                send(&sender, &context, Event::GameStarted(pid, path.to_owned()));
            });
            let event = if started {
                Event::GameExited(result)
            } else {
                Event::Failed(result.err().unwrap_or_else(|| "启动失败".into()))
            };
            send(&sender, &context, event);
        });
    }
}

fn reporter(sender: &Sender<Event>, context: &egui::Context) -> Reporter {
    let sender = sender.clone();
    let context = context.clone();
    Arc::new(move |progress| send(&sender, &context, Event::Progress(progress)))
}

fn send(sender: &Sender<Event>, context: &egui::Context, event: Event) {
    let _ = sender.send(event);
    context.request_repaint();
}
