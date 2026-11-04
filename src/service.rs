use crate::{
    minecraft::{
        self, Downloader, InstalledVersion, LoaderKind, LoaderVersion, Progress, Reporter,
        VersionEntry,
    },
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

pub mod backup;
#[cfg(test)]
mod backup_tests;
pub mod content;
mod pack_import;
pub use pack_import::{PackImport, PackImportStatus};
mod downloads;
mod export;
#[cfg(test)]
mod isolation_tests;
#[cfg(test)]
mod java_tests;
#[cfg(test)]
mod managed_tests;
mod manual;
mod migration;
pub mod mod_tools;
mod optifine;
pub mod pack_updates;
mod resources;
pub use content::{ContentPlan, ContentState};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Idle,
    Manifest,
    LoaderVersions,
    Install,
    Launch,
    Content,
}

#[derive(Clone)]
pub enum LaunchPrompt {
    Account,
    Java {
        instance: Box<crate::instances::Instance>,
        major: u32,
        detail: String,
    },
}

enum Event {
    ExternalSession(Box<nscl::external_auth::Session>),
    Migrated(crate::instances::Instance),
    ModTools(mod_tools::ModEvent),
    Pack(pack_updates::PackEvent),
    LaunchPrompt(LaunchPrompt),
    Progress(Progress),
    Versions(Vec<VersionEntry>),
    Loaders(Vec<LoaderVersion>),
    Installed(InstalledVersion),
    Failed(String),
    GameStarted(u32, PathBuf),
    GameExited(std::result::Result<i32, String>),
    Session(crate::accounts::Session),
    Java(Vec<crate::java::JavaRuntime>),
    JavaChecked(Result<crate::java::JavaRuntime, String>),
    JavaInstalled(crate::java::JavaRuntime),
    Content(content::ContentEvent),
    PackExported(PathBuf),
    Local(resources::LocalEvent),
    Backup(backup::BackupEvent),
}

pub struct GameService {
    pub active_instance: String,
    pub identify_plan: Option<crate::content::IdentifyPlan>,
    pub pack_state: pack_updates::PackState,
    pub queue: crate::download_tasks::Queue,
    pub rate: crate::download_tasks::Rate,
    paused: Arc<AtomicBool>,
    pub launch_prompt: Option<LaunchPrompt>,
    pub pending_java_launch: Option<String>,
    pub resume_launch: Option<String>,
    pub versions: Vec<VersionEntry>,
    pub installed: Vec<InstalledVersion>,
    pub selected: String,
    pub search: String,
    pub snapshots: bool,
    pub loader_search: String,
    pub loaders: Vec<LoaderVersion>,
    pub progress: Progress,
    pub pack_import: Option<PackImport>,
    pub error: Option<String>,
    pub manifest_error: Option<String>,
    pub notice: String,
    pub task: Task,
    pub game: Option<(u32, PathBuf)>,
    pub last_log: Option<PathBuf>,
    pub game_control: minecraft::GameControl,
    pub show_diagnostics: bool,
    pub backup_snapshots: Vec<crate::instances::backup::Snapshot>,
    pub backup_instance: String,
    pub instances: Vec<crate::instances::Instance>,
    pub renewed_session: Option<crate::accounts::Session>,
    pub renewed_external: Option<nscl::external_auth::Session>,
    pub java_runtimes: Vec<crate::java::JavaRuntime>,
    pub java_busy: bool,
    pub java_status: String,
    pub java_install_major: u32,
    pub content: ContentState,
    pub exported_pack: Option<PathBuf>,
    pub local: resources::LocalState,
    versions_requested: bool,
    loaders_requested: Option<(String, LoaderKind)>,
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
            active_instance: String::new(),
            identify_plan: None,
            pack_state: Default::default(),
            queue: crate::download_tasks::Queue::load(&store::game_directory(settings)),
            rate: Default::default(),
            paused: settings.source.paused.clone(),
            launch_prompt: None,
            pending_java_launch: None,
            resume_launch: None,
            versions: Vec::new(),
            installed,
            selected,
            search: String::new(),
            snapshots: false,
            loader_search: String::new(),
            loaders: Vec::new(),
            progress: Progress::default(),
            error: None,
            manifest_error: None,
            notice: String::new(),
            task: Task::Idle,
            game: None,
            last_log: None,
            game_control: minecraft::GameControl::default(),
            show_diagnostics: false,
            backup_snapshots: Vec::new(),
            backup_instance: String::new(),
            instances,
            renewed_session: None,
            renewed_external: None,
            java_runtimes: Vec::new(),
            java_busy: false,
            java_status: String::new(),
            java_install_major: 21,
            content: ContentState::default(),
            pack_import: None,
            exported_pack: None,
            local: resources::LocalState::default(),
            versions_requested: false,
            loaders_requested: None,
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

    pub fn stop_game(&mut self, force: bool) {
        if self.game.is_some() {
            self.game_control.request_stop(force);
            self.notice = if force {
                "正在结束当前游戏进程……"
            } else {
                "正在请求游戏正常关闭……"
            }
            .into();
        }
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
        self.paused.store(false, Ordering::Relaxed);
        self.rate = Default::default();
        self.cancel = Arc::new(AtomicBool::new(false));
        self.task = task;
        self.error = None;
        self.manifest_error = None;
        self.notice.clear();
        self.progress = Progress::default();
    }

    pub fn poll(&mut self, settings: &Settings) {
        if let Some(notice) = self.game_control.take_notice() {
            self.notice = notice;
        }
        for _ in 0..1024 {
            let Ok(event) = self.receiver.try_recv() else {
                break;
            };
            match event {
                Event::ModTools(event) => self.apply_mod_tools(event),
                Event::Pack(event) => self.apply_pack(event, settings),
                Event::LaunchPrompt(prompt) => {
                    self.task = Task::Idle;
                    self.launch_prompt = Some(prompt);
                }
                Event::Progress(progress) => {
                    self.rate.update(progress.bytes_downloaded);
                    self.progress = progress;
                }
                Event::Versions(versions) => {
                    self.versions = versions;
                    self.task = Task::Idle;
                    self.notice.clear();
                }
                Event::Loaders(versions) => {
                    self.loaders = versions;
                    self.task = Task::Idle;
                    self.notice.clear();
                }
                Event::Installed(version) => {
                    self.queue
                        .finish(crate::download_tasks::State::Complete, "安装完成".into());
                    let isolation = self
                        .instances
                        .iter()
                        .find(|i| i.version_id == version.id)
                        .map(|i| i.isolation)
                        .unwrap_or(settings.default_isolation);
                    match crate::instances::ensure_default(
                        &store::game_directory(settings),
                        &version.id,
                        isolation,
                    ) {
                        Ok(instance) => self.selected = instance.id,
                        Err(error) => self.error = Some(error),
                    }
                    self.refresh(settings);
                    self.task = Task::Idle;
                    self.notice = format!(
                        "{} 安装完成，可在开始页启动。",
                        version
                            .loader
                            .as_ref()
                            .map(LoaderVersion::label)
                            .unwrap_or_else(|| format!("Minecraft {}", version.id))
                    );
                }
                Event::Failed(error) => {
                    self.queue.finish(
                        if error == "操作已取消。" {
                            crate::download_tasks::State::Cancelled
                        } else {
                            crate::download_tasks::State::Failed
                        },
                        error.clone(),
                    );
                    let failed_task = self.task;
                    self.task = Task::Idle;
                    if error == "操作已取消。" {
                        self.manifest_error = None;
                        self.notice = error;
                    } else {
                        self.manifest_error =
                            (failed_task == Task::Manifest).then(|| error.clone());
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
                    self.active_instance.clear();
                    self.game = None;
                    if self.game_control.stopped() {
                        self.notice = "游戏已停止。".into();
                        continue;
                    }
                    self.show_diagnostics = !matches!(result, Ok(0));
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
                Event::ExternalSession(session) => self.renewed_external = Some(*session),
                Event::Migrated(instance) => {
                    self.task = Task::Idle;
                    self.selected = instance.id;
                    self.refresh(settings);
                    self.notice = "实例已复制迁入；来源文件保持不变。".into();
                }
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
                Event::JavaInstalled(runtime) => {
                    if let Some(id) = self.pending_java_launch.take() {
                        self.resume_launch = Some(id);
                        self.launch_prompt = None;
                    }
                    self.task = Task::Idle;
                    self.java_status = format!("已安装：{}，自动匹配可直接使用。", runtime.label());
                    self.notice = self.java_status.clone();
                    self.java_runtimes.retain(|r| r.path != runtime.path);
                    self.java_runtimes.push(runtime);
                }
                Event::Content(event) => self.apply_content(event, settings),
                Event::PackExported(path) => {
                    self.task = Task::Idle;
                    self.notice = format!("整合包已导出：{}", path.display());
                    self.exported_pack = Some(path);
                }
                Event::Local(event) => self.apply_local(event),
                Event::Backup(event) => self.apply_backup(event, settings),
            }
            self.finish_pack_import();
        }
    }

    pub fn invalidate_versions(&mut self) {
        self.versions.clear();
        self.versions_requested = false;
        self.loaders.clear();
        self.loaders_requested = None;
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
        self.start_install(entry, None, settings, ctx);
    }

    pub fn ensure_loaders_loaded(
        &mut self,
        minecraft: &str,
        kind: LoaderKind,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.loaders_requested.as_ref() != Some(&(minecraft.to_owned(), kind)) {
            self.load_loaders(minecraft, kind, settings, ctx);
        }
    }

    pub fn load_loaders(
        &mut self,
        minecraft: &str,
        kind: LoaderKind,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || minecraft.is_empty() {
            return;
        }
        let minecraft = minecraft.to_owned();
        self.loaders_requested = Some((minecraft.clone(), kind));
        self.loaders.clear();
        self.begin(Task::LoaderVersions);
        let (source, cancel, sender, context) = (
            settings.source.clone(),
            self.cancel.clone(),
            self.sender.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(source, cancel, reporter(&sender, &context))
                .and_then(|downloader| minecraft::loader_versions(&minecraft, kind, &downloader));
            send(
                &sender,
                &context,
                match result {
                    Ok(versions) => Event::Loaders(versions),
                    Err(error) => Event::Failed(error),
                },
            );
        });
    }

    pub fn install_modded(
        &mut self,
        selection: LoaderVersion,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        let Some(entry) = self
            .versions
            .iter()
            .find(|entry| entry.id == selection.minecraft)
            .cloned()
        else {
            self.error = Some("Minecraft 版本不存在，请刷新版本列表。".into());
            return;
        };
        self.start_install(entry, Some(selection), settings, ctx);
    }

    fn start_install(
        &mut self,
        entry: VersionEntry,
        loader: Option<LoaderVersion>,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() {
            return;
        }
        if self.game.is_some() {
            self.error = Some("请先退出当前游戏，再安装或修复版本。".into());
            return;
        }
        self.begin(Task::Install);
        let root = store::game_directory(settings);
        if self.queue.active.is_none() {
            self.queue.enqueue(entry.clone(), loader.clone(), true);
        }
        let source = settings.source.clone();
        let java = settings.configured_java().to_owned();
        let cancel = self.cancel.clone();
        let sender = self.sender.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let reporter = reporter(&sender, &context);
            let result =
                Downloader::new(source, cancel, reporter).and_then(|downloader| match loader {
                    Some(loader) => {
                        minecraft::install_loader(&root, &entry, &loader, &java, &downloader)
                    }
                    None => minecraft::install(&root, &entry, &downloader),
                });
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

    pub fn install_java(&mut self, ctx: &egui::Context) {
        if self.busy() || self.java_busy {
            return;
        }
        self.begin(Task::Install);
        let (major, sender, cancel, ctx) = (
            self.java_install_major,
            self.sender.clone(),
            self.cancel.clone(),
            ctx.clone(),
        );
        let source = crate::minecraft::Source {
            paused: self.paused.clone(),
            ..Default::default()
        };
        std::thread::spawn(move || {
            let result = Downloader::new(source, cancel, reporter(&sender, &ctx))
                .and_then(|d| crate::java::install_runtime(major, &d));
            send(
                &sender,
                &ctx,
                match result {
                    Ok(runtime) => Event::JavaInstalled(runtime),
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
        let Some(selected_account) = accounts.list.current() else {
            self.launch_prompt = Some(LaunchPrompt::Account);
            return;
        };
        let account = accounts.launch_account();
        let external = accounts.external_for_launch();
        let offline_identity = minecraft::LaunchIdentity::offline(&selected_account.name);
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
        self.active_instance = instance.id.clone();
        self.game_control = minecraft::GameControl::default();
        self.show_diagnostics = false;
        let control = self.game_control.clone();
        self.progress.stage = "校验游戏文件、选择 Java 并准备启动".into();
        let root = store::game_directory(settings);
        let settings = settings.clone();
        let sender = self.sender.clone();
        let context = ctx.clone();
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            let effective = instance.effective_settings(&settings);
            let required = minecraft::installed_versions(&root)
                .into_iter()
                .find(|v| v.id == instance.version_id)
                .map(|v| v.java_major)
                .unwrap_or(8);
            if let Err(e) = instance
                .game_directory(&root)
                .and_then(|p| minecraft::preflight::directory(&p))
            {
                send(&sender, &context, Event::Failed(e));
                return;
            }
            if let Err(detail) = crate::java::find(effective.configured_java(), required) {
                send(
                    &sender,
                    &context,
                    Event::LaunchPrompt(LaunchPrompt::Java {
                        instance: Box::new(instance),
                        major: required,
                        detail,
                    }),
                );
                return;
            }
            let identity = if let Some((account, cached)) = external {
                let result = nscl::external_auth::Client::new()
                    .and_then(|client| {
                        client.resume(&account, cached, &nscl::external_auth::SystemVault, &cancel)
                    })
                    .and_then(|session| {
                        send(
                            &sender,
                            &context,
                            Event::ExternalSession(Box::new(session.clone())),
                        );
                        let d = Downloader::new(
                            settings.source.clone(),
                            cancel.clone(),
                            reporter(&sender, &context),
                        )?;
                        let agent = nscl::external_auth::install_agent(
                            &store::settings_path().with_file_name("tools"),
                            &d,
                        )?;
                        minecraft::LaunchIdentity::external(&session, &agent)
                    });
                match result {
                    Ok(identity) => identity,
                    Err(e) => {
                        send(&sender, &context, Event::Failed(e));
                        return;
                    }
                }
            } else {
                match account {
                    Some((account, cached)) => {
                        match crate::accounts::AuthClient::new().and_then(|client| {
                            client.resume(&account, cached, &crate::accounts::SystemVault, &cancel)
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
                    None => offline_identity,
                }
            };
            let mut plan = match Downloader::new(
                settings.source.clone(),
                cancel.clone(),
                reporter(&sender, &context),
            )
            .and_then(|downloader| {
                minecraft::prepare_instance_launch_with_downloader(
                    &root,
                    &instance,
                    &settings,
                    &identity,
                    &downloader,
                )
            }) {
                Ok(plan) => plan,
                Err(error) => {
                    send(&sender, &context, Event::Failed(error));
                    return;
                }
            };
            if cancel.load(Ordering::Relaxed) {
                send(&sender, &context, Event::Failed("操作已取消。".into()));
                return;
            }
            let mut started = false;
            let session = match minecraft::session::LaunchSession::begin(&plan.game_directory) {
                Ok(session) => session,
                Err(error) => {
                    send(
                        &sender,
                        &context,
                        Event::Failed(format!("创建启动会话失败：{error}")),
                    );
                    return;
                }
            };
            plan.log_path = session.log_path();
            let result = minecraft::launch_controlled(
                &plan,
                |pid, path| {
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
                },
                &control,
            );
            let result = match session.finish() {
                Ok(()) => result,
                Err(error) if started => Err(format!("游戏已退出，但会话日志归档失败：{error}")),
                Err(_) => result,
            };
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

#[cfg(test)]
mod drop_tests;
#[cfg(test)]
pub(crate) mod tests;
