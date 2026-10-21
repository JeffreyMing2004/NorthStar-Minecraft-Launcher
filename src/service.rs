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
    Session(crate::accounts::Session),
    Java(Vec<crate::java::JavaRuntime>),
    JavaChecked(Result<crate::java::JavaRuntime, String>),
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
    pub instances: Vec<crate::instances::Instance>,
    pub renewed_session: Option<crate::accounts::Session>,
    pub java_runtimes: Vec<crate::java::JavaRuntime>,
    pub java_busy: bool,
    pub java_status: String,
    versions_requested: bool,
    cancel: Arc<AtomicBool>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}

impl GameService {
    pub fn new(settings: &Settings) -> Self {
        let (sender, receiver) = mpsc::channel();
        let installed = minecraft::installed_versions(&store::game_directory(settings));
        let instances = crate::instances::list(&store::game_directory(settings));
        let selected = instances
            .iter()
            .find(|i| i.id == settings.selected_instance)
            .or_else(|| instances.first())
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
            instances,
            renewed_session: None,
            java_runtimes: Vec::new(),
            java_busy: false,
            java_status: String::new(),
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
        self.instances = crate::instances::list(&store::game_directory(settings));
        if !self.instances.iter().any(|entry| entry.id == self.selected) {
            self.selected = self
                .instances
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
                    if !self.instances.iter().any(|i| i.version_id == version.id) {
                        match crate::instances::create(
                            &store::game_directory(settings),
                            &version.id,
                            &format!("Minecraft {}", version.id),
                        ) {
                            Ok(instance) => {
                                self.selected = instance.id;
                                self.refresh(settings);
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
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
                Event::Session(session) => self.renewed_session = Some(session),
                Event::Java(runtimes) => {
                    self.java_status = format!("检测到 {} 个 Java 运行环境。", runtimes.len());
                    self.java_runtimes = runtimes;
                    self.java_busy = false;
                }
                Event::JavaChecked(result) => {
                    self.java_busy = false;
                    match result {
                        Ok(runtime) => {
                            self.java_status = format!("检测通过：{}", runtime.label());
                            self.java_runtimes.retain(|r| r.path != runtime.path);
                            self.java_runtimes.push(runtime);
                        }
                        Err(error) => self.java_status = error,
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

    pub fn inspect_java(&mut self, path: Option<String>, ctx: &egui::Context) {
        if self.java_busy {
            return;
        }
        self.java_busy = true;
        self.java_status = "正在检测 Java……".into();
        let (sender, context) = (self.sender.clone(), ctx.clone());
        std::thread::spawn(move || {
            let event = match path {
                Some(path) => Event::JavaChecked(crate::java::probe(std::path::Path::new(
                    path.trim().trim_matches('"'),
                ))),
                None => Event::Java(crate::java::discover()),
            };
            send(&sender, &context, event);
        });
    }

    pub fn launch(
        &mut self,
        settings: &Settings,
        accounts: &crate::account_service::AccountService,
        ctx: &egui::Context,
    ) {
        if self.busy() || self.game.is_some() || self.selected.is_empty() {
            return;
        }
        if accounts.pending {
            self.error = Some("请先完成或取消账号登录。".into());
            return;
        }
        let account = if settings.online_mode {
            match accounts.launch_account() {
                Some(account) => Some(account),
                None => {
                    self.error =
                        Some("请先在账号页登录 Microsoft 账号，或明确选择离线测试身份。".into());
                    return;
                }
            }
        } else {
            None
        };
        let Some(instance) = self
            .instances
            .iter()
            .find(|i| i.id == self.selected)
            .cloned()
        else {
            self.error = Some("所选实例不存在，请刷新实例列表。".into());
            return;
        };
        self.begin(Task::Launch);
        self.progress.stage = "校验游戏文件、选择 Java 并准备启动".into();
        let root = store::game_directory(settings);
        let settings = settings.clone();
        let sender = self.sender.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let identity = match account {
                Some((account, cached)) => {
                    match crate::accounts::AuthClient::new().and_then(|client| {
                        client.resume(
                            &account,
                            cached,
                            &crate::accounts::SystemVault,
                            &AtomicBool::new(false),
                        )
                    }) {
                        Ok(session) => {
                            let identity = minecraft::LaunchIdentity::microsoft(&session);
                            send(&sender, &context, Event::Session(session));
                            identity
                        }
                        Err(error) => {
                            send(
                                &sender,
                                &context,
                                Event::Failed(format!("正版身份验证失败：{error}")),
                            );
                            return;
                        }
                    }
                }
                None => minecraft::LaunchIdentity::offline(&settings.player_name),
            };
            let plan =
                match minecraft::prepare_instance_launch(&root, &instance, &settings, &identity) {
                    Ok(plan) => plan,
                    Err(error) => {
                        send(&sender, &context, Event::Failed(error));
                        return;
                    }
                };
            let mut started = false;
            let result = minecraft::launch(&plan, |pid, path| {
                started = true;
                let mut instance = instance.clone();
                instance.last_played = Some(crate::accounts::now());
                if let Err(error) = crate::instances::save(&root, &instance) {
                    send(
                        &sender,
                        &context,
                        Event::Progress(Progress {
                            stage: format!("游戏已启动，实例记录保存失败：{error}"),
                            ..Default::default()
                        }),
                    );
                }
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
