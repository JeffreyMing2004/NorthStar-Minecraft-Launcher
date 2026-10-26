use super::{Event, GameService, Settings, Task, reporter, send};
use crate::{
    content::{
        self, CompatibilityReport, ContentKind, ContentVersion, ModPlan, Modrinth, PackPlan,
        Project, SearchPage, Target,
    },
    instances::Instance,
    minecraft::{Downloader, Result},
    store,
};
use std::path::PathBuf;

#[derive(Clone)]
pub enum ContentPlan {
    Mod(ModPlan),
    Pack(PackPlan),
    Remove(content::RemovalPlan),
}

/// Which surface owns the resource browser.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum ContentMode {
    /// The download page browses Modrinth on its own and saves mod files to disk.
    #[default]
    Download,
    /// The instance mod tab filters by that instance and installs into it.
    Instance,
}

#[derive(Default)]
pub struct ContentState {
    pub kind: ContentKind,
    pub mode: ContentMode,
    pub query: String,
    /// Instance bound to [`ContentMode::Instance`]; empty on the download page.
    pub target: String,
    /// The instance tab shows the Modrinth search instead of the installed list.
    pub adding: bool,
    /// The installed catalog for `target` was already requested, so rendering never loops.
    pub managed_requested: bool,
    pub downloaded: Option<PathBuf>,
    pub results: SearchPage,
    pub searched: bool,
    pub project: Option<Project>,
    pub versions: Vec<ContentVersion>,
    pub plan: Option<ContentPlan>,
    pub audit: Option<CompatibilityReport>,
    pub managed: Option<content::ManagedCatalog>,
    pub updates: content::UpdateReport,
    pub updates_checked: bool,
    pub managed_selection: std::collections::BTreeSet<String>,
}

impl ContentState {
    fn scoped(kind: ContentKind, mode: ContentMode, target: String) -> Self {
        Self {
            kind,
            mode,
            target,
            ..Default::default()
        }
    }

    fn accepts_plan(&self, plan: &ContentPlan) -> bool {
        match plan {
            ContentPlan::Pack(_) => {
                self.mode == ContentMode::Download && self.kind == ContentKind::Modpack
            }
            ContentPlan::Mod(plan) => {
                self.mode == ContentMode::Instance
                    && self.kind == ContentKind::Mod
                    && self.target == plan.target.instance.id
            }
            ContentPlan::Remove(plan) => {
                self.mode == ContentMode::Instance
                    && self.kind == ContentKind::Mod
                    && self.target == plan.target.instance.id
            }
        }
    }

    pub fn download_pack_plan(&self) -> Option<&PackPlan> {
        match self.plan.as_ref()? {
            ContentPlan::Pack(plan)
                if self.mode == ContentMode::Download && self.kind == ContentKind::Modpack =>
            {
                Some(plan)
            }
            _ => None,
        }
    }
}

pub(super) enum ContentEvent {
    Search(SearchPage),
    Versions(Vec<ContentVersion>),
    Plan(Box<ContentPlan>),
    Audit(CompatibilityReport),
    Installed(Instance, String),
    Managed(Box<content::ManagedCatalog>, Option<content::UpdateReport>),
    Downloaded(PathBuf),
}

impl GameService {
    pub fn content_target(&self, settings: &Settings) -> Result<Target> {
        let instance = self
            .instances
            .iter()
            .find(|i| i.id == self.content.target)
            .ok_or("请先选择要管理模组的实例。")?;
        Target::from_instance(&store::game_directory(settings), instance)
    }

    /// Point the browser at the download page: no instance, no loader filter.
    pub fn content_browse(&mut self, kind: ContentKind) {
        if self.busy() || (self.content.kind == kind && self.content.mode == ContentMode::Download)
        {
            return;
        }
        self.content = ContentState::scoped(kind, ContentMode::Download, String::new());
    }

    /// Point the browser at one instance. Repeating a frame keeps the loaded catalog.
    pub fn content_instance(&mut self, id: &str) {
        if self.busy() || (self.content.mode == ContentMode::Instance && self.content.target == id)
        {
            return;
        }
        self.content = ContentState::scoped(ContentKind::Mod, ContentMode::Instance, id.to_owned());
    }

    pub fn content_search(&mut self, offset: usize, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let kind = self.content.kind;
        let target = match self.content.mode {
            ContentMode::Download => None,
            ContentMode::Instance => match self.content_target(settings) {
                Ok(target) if target.loader.is_some() => Some(target),
                Ok(_) => {
                    self.error=Some("请先在 Minecraft 下载页为目标游戏安装 Forge、Fabric 或 NeoForge，再选择对应实例。".into());
                    return;
                }
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            },
        };
        let query = self.content.query.clone();
        self.content.project = None;
        self.content.versions.clear();
        self.content.results = SearchPage::default();
        self.content.searched = true;
        self.begin(Task::Content);
        let (settings, ctx, sender, cancel) = (
            settings.clone(),
            ctx.clone(),
            self.sender.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(settings.source, cancel, reporter(&sender, &ctx))
                .and_then(|d| {
                    d.phase("搜索 Modrinth 资源", 0);
                    Modrinth::new(d).search(kind, &query, offset, target.as_ref())
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(page) => Event::Content(ContentEvent::Search(page)),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }

    /// True when the browser filters by an instance instead of listing everything.
    fn content_targets_instance(&self) -> bool {
        self.content.kind == ContentKind::Mod && self.content.mode == ContentMode::Instance
    }

    pub fn content_versions(&mut self, project: Project, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let target = if self.content_targets_instance() {
            match self.content_target(settings) {
                Ok(t) => Some(t),
                Err(e) => {
                    self.error = Some(e);
                    return;
                }
            }
        } else {
            None
        };
        let kind = self.content.kind;
        self.content.project = Some(project.clone());
        self.content.versions.clear();
        self.begin(Task::Content);
        let (settings, ctx, sender, cancel) = (
            settings.clone(),
            ctx.clone(),
            self.sender.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(settings.source, cancel, reporter(&sender, &ctx))
                .and_then(|d| {
                    d.phase("获取资源版本", 0);
                    let api = Modrinth::new(d);
                    api.check_project(&project.project_id, kind)?;
                    api.versions(&project.project_id, target.as_ref())
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(v) => Event::Content(ContentEvent::Versions(v)),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }

    pub fn content_preview(
        &mut self,
        version: ContentVersion,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() {
            return;
        }
        if self.content.kind == ContentKind::Mod && self.content.mode != ContentMode::Instance {
            self.error = Some("请在版本管理中为具体实例安装模组，或直接下载模组文件。".into());
            return;
        }
        let target = if self.content.kind == ContentKind::Mod {
            match self.content_target(settings) {
                Ok(t) => Some(t),
                Err(e) => {
                    self.error = Some(e);
                    return;
                }
            }
        } else {
            None
        };
        self.begin(Task::Content);
        let (settings, ctx, sender, cancel) = (
            settings.clone(),
            ctx.clone(),
            self.sender.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let root = store::game_directory(&settings);
            let result = Downloader::new(settings.source, cancel, reporter(&sender, &ctx))
                .and_then(|d| {
                    d.phase("解析依赖并检查兼容性", 0);
                    let api = Modrinth::new(d);
                    let fresh = api.version(&version.id)?;
                    if fresh.project_id != version.project_id {
                        return Err("资源项目 ID 已改变，请重新选择。".into());
                    }
                    match target {
                        Some(target) => {
                            content::plan_mod(&root, target, fresh, &api).map(ContentPlan::Mod)
                        }
                        None => content::plan_pack(&root, &fresh, &api).map(ContentPlan::Pack),
                    }
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(p) => Event::Content(ContentEvent::Plan(Box::new(p))),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }

    /// Save one mod JAR under the launcher download directory without touching an instance.
    pub fn content_download(
        &mut self,
        version: ContentVersion,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || self.content.kind != ContentKind::Mod {
            return;
        }
        let directory = store::settings_path()
            .with_file_name("downloads")
            .join("mods");
        self.begin(Task::Content);
        self.content.downloaded = None;
        let (settings, ctx, sender, cancel) = (
            settings.clone(),
            ctx.clone(),
            self.sender.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(settings.source, cancel, reporter(&sender, &ctx))
                .and_then(|d| {
                    let api = Modrinth::new(d);
                    api.check_project(&version.project_id, ContentKind::Mod)?;
                    let fresh = api.version(&version.id)?;
                    if fresh.project_id != version.project_id {
                        return Err("资源项目 ID 已改变，请重新选择。".into());
                    }
                    content::download_mod(&directory, &fresh, &api.downloader)
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(path) => Event::Content(ContentEvent::Downloaded(path)),
                    Err(error) => Event::Failed(error),
                },
            );
        });
    }

    pub fn content_drop_install(
        &mut self,
        path: &std::path::Path,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || self.game.is_some() {
            return;
        }
        self.content =
            ContentState::scoped(ContentKind::Modpack, ContentMode::Download, String::new());
        self.begin(Task::Content);
        self.progress.stage = "正在识别拖入的整合包".into();
        let (path, settings, ctx, sender, cancel) = (
            path.to_path_buf(),
            settings.clone(),
            ctx.clone(),
            self.sender.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let root = store::game_directory(&settings);
            let result = Downloader::new(settings.source.clone(), cancel, reporter(&sender, &ctx))
                .and_then(|d| {
                    d.check_cancelled()?;
                    let plan = content::inspect_pack(&root, &path)?;
                    d.check_cancelled()?;
                    let instance = content::install_pack(&root, &plan, &settings, &d)?;
                    Ok((
                        instance,
                        format!("整合包 {} 安装完成，可在开始页启动。", plan.name),
                    ))
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok((instance, message)) => {
                        Event::Content(ContentEvent::Installed(instance, message))
                    }
                    Err(error) => Event::Failed(error),
                },
            );
        });
    }

    /// The download-page confirmation can only execute a pack installation.
    pub fn content_install_pack(&mut self, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        if self.content.download_pack_plan().is_none() {
            self.error = Some("当前方案不是下载页的整合包安装方案，请重新选择。".into());
            return;
        }
        self.content_install(settings, ctx);
    }

    pub fn content_install(&mut self, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        if self.game.is_some() {
            self.error = Some("请先退出当前游戏，再安装模组或整合包。".into());
            return;
        }
        let Some(plan) = self.content.plan.clone() else {
            return;
        };
        if !self.content.accepts_plan(&plan) {
            self.error = Some("方案不属于当前页面或实例，请返回对应实例重新生成方案。".into());
            return;
        }
        self.begin(Task::Content);
        let (settings, ctx, sender, cancel) = (
            settings.clone(),
            ctx.clone(),
            self.sender.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let root = store::game_directory(&settings);
            let result = Downloader::new(settings.source.clone(), cancel, reporter(&sender, &ctx))
                .and_then(|d| match plan {
                    ContentPlan::Mod(plan) => content::install_mods(&root, &plan, &d)
                        .map(|i| (i, format!("{} 个模组及依赖安装完成。", plan.versions.len()))),
                    ContentPlan::Remove(plan) => content::remove_mods(&root, &plan, &d).map(|i| {
                        (
                            i,
                            format!("已卸载 {} 个模组，存档与配置已保留。", plan.entries.len()),
                        )
                    }),
                    ContentPlan::Pack(plan) => content::install_pack(&root, &plan, &settings, &d)
                        .map(|i| {
                            (
                                i,
                                format!("整合包 {} 安装完成，可在开始页启动。", plan.name),
                            )
                        }),
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok((i, message)) => Event::Content(ContentEvent::Installed(i, message)),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }

    pub fn content_manage(&mut self, settings: &Settings, ctx: &egui::Context, updates: bool) {
        if self.busy() {
            return;
        }
        // Recorded before the target is resolved so a failing instance never retries every frame.
        self.content.managed_requested = true;
        let target = match self.content_target(settings) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        self.begin(Task::Content);
        let (settings, sender, cancel, ctx) = (
            settings.clone(),
            self.sender.clone(),
            self.cancel.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let root = store::game_directory(&settings);
            let result = Downloader::new(settings.source, cancel, reporter(&sender, &ctx))
                .and_then(|d| {
                    d.phase("读取已安装模组并检查文件", 0);
                    let catalog = content::list_managed(&root, &target, &d)?;
                    let report = if updates {
                        Some(content::check_updates(&catalog, &Modrinth::new(d))?)
                    } else {
                        None
                    };
                    Ok((catalog, report))
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok((list, report)) => {
                        Event::Content(ContentEvent::Managed(Box::new(list), report))
                    }
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }

    pub fn content_remove_preview(&mut self, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let target = match self.content_target(settings) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let (root, projects) = (
            store::game_directory(settings),
            self.content.managed_selection.clone(),
        );
        self.begin(Task::Content);
        let (sender, ctx, cancel) = (self.sender.clone(), ctx.clone(), self.cancel.clone());
        std::thread::spawn(move || {
            let result = content::plan_removal(&root, &target, &projects).and_then(|plan| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    Err("操作已取消。".into())
                } else {
                    Ok(plan)
                }
            });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(plan) => {
                        Event::Content(ContentEvent::Plan(Box::new(ContentPlan::Remove(plan))))
                    }
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }

    pub fn content_check(&mut self, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let target = match self.content_target(settings) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        self.begin(Task::Content);
        let (settings, ctx, sender, cancel) = (
            settings.clone(),
            ctx.clone(),
            self.sender.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let root = store::game_directory(&settings);
            let result = Downloader::new(settings.source, cancel, reporter(&sender, &ctx))
                .and_then(|d| {
                    d.phase("检查实例模组版本、依赖和文件", 0);
                    content::check_instance(&root, &target.instance, &d)
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(r) => Event::Content(ContentEvent::Audit(r)),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }

    pub(super) fn apply_content(&mut self, event: ContentEvent, settings: &Settings) {
        self.task = Task::Idle;
        if self.cancel.load(std::sync::atomic::Ordering::Relaxed)
            && !matches!(event, ContentEvent::Installed(..))
        {
            self.notice = "操作已取消。".into();
            return;
        }
        match event {
            ContentEvent::Search(page) => self.content.results = page,
            ContentEvent::Versions(versions) => self.content.versions = versions,
            ContentEvent::Plan(plan) => self.content.plan = Some(*plan),
            ContentEvent::Audit(report) => self.content.audit = Some(report),
            ContentEvent::Installed(instance, message) => {
                self.content.plan = None;
                self.content.project = None;
                self.content.versions.clear();
                self.content.managed = None;
                self.content.managed_selection.clear();
                self.content.updates = Default::default();
                self.content.updates_checked = false;
                self.content.managed_requested = false;
                self.content.adding = false;
                self.refresh(settings);
                self.selected = instance.id.clone();
                self.content.target = instance.id;
                self.notice = message;
            }
            ContentEvent::Downloaded(path) => {
                self.notice = "模组已下载，可放入实例的 mods 目录。".into();
                self.content.downloaded = Some(path);
            }
            ContentEvent::Managed(catalog, report) => {
                self.content.managed_selection.retain(|id| {
                    catalog
                        .entries
                        .iter()
                        .any(|m| &m.installed.version.project_id == id)
                });
                self.content.managed = Some(*catalog);
                self.content.updates_checked = report.is_some();
                self.content.updates = report.unwrap_or_default();
            }
        }
    }
}
