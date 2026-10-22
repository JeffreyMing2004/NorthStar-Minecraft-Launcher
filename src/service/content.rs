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

#[derive(Clone)]
pub enum ContentPlan {
    Mod(ModPlan),
    Pack(PackPlan),
    Remove(content::RemovalPlan),
}

#[derive(Default)]
pub struct ContentState {
    pub kind: ContentKind,
    pub query: String,
    pub target: String,
    pub results: SearchPage,
    pub searched: bool,
    pub project: Option<Project>,
    pub versions: Vec<ContentVersion>,
    pub plan: Option<ContentPlan>,
    pub audit: Option<CompatibilityReport>,
    pub local_path: String,
    pub manager_open: bool,
    pub managed: Option<content::ManagedCatalog>,
    pub updates: content::UpdateReport,
    pub updates_checked: bool,
    pub managed_selection: std::collections::BTreeSet<String>,
}

pub(super) enum ContentEvent {
    Search(SearchPage),
    Versions(Vec<ContentVersion>),
    Plan(Box<ContentPlan>),
    Audit(CompatibilityReport),
    Installed(Instance, String),
    Managed(Box<content::ManagedCatalog>, Option<content::UpdateReport>),
}

impl GameService {
    pub fn content_target(&self, settings: &Settings) -> Result<Target> {
        let instance = self
            .instances
            .iter()
            .find(|i| i.id == self.content.target)
            .ok_or("请先选择要安装模组的实例。")?;
        Target::from_instance(&store::game_directory(settings), instance)
    }

    pub fn content_kind(&mut self, kind: ContentKind) {
        if self.content.kind != kind && !self.busy() {
            self.content = ContentState {
                kind,
                target: self.content.target.clone(),
                ..Default::default()
            };
        }
        if self.content.target.is_empty() {
            self.content.target = self.selected.clone();
        }
    }

    pub fn content_search(&mut self, offset: usize, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let kind = self.content.kind;
        let target = if kind == ContentKind::Mod {
            match self.content_target(settings) {
                Ok(target) if target.loader.is_some() => Some(target),
                Ok(_) => {
                    self.error=Some("请先在 Minecraft 下载页为目标游戏安装 Forge、Fabric 或 NeoForge，再选择对应实例。".into());
                    return;
                }
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
        } else {
            None
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

    pub fn content_versions(&mut self, project: Project, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
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

    pub fn content_import(&mut self, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let path = std::path::PathBuf::from(self.content.local_path.trim().trim_matches('"'));
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("mrpack") || e.eq_ignore_ascii_case("zip"))
        {
            self.error = Some("请选择 .mrpack 或 Modrinth 格式的 ZIP 整合包。".into());
            return;
        }
        self.begin(Task::Content);
        let (root, ctx, sender, cancel) = (
            store::game_directory(settings),
            ctx.clone(),
            self.sender.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = content::inspect_pack(&root, &path).and_then(|p| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    Err("操作已取消。".into())
                } else {
                    Ok(p)
                }
            });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(p) => Event::Content(ContentEvent::Plan(Box::new(ContentPlan::Pack(p)))),
                    Err(e) => Event::Failed(e),
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
        self.content = ContentState {
            kind: ContentKind::Modpack,
            local_path: path.to_string_lossy().into_owned(),
            ..Default::default()
        };
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
        let target = match self.content_target(settings) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        self.content.manager_open = true;
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
            ContentEvent::Plan(plan) => {
                self.content.plan = Some(*plan);
                self.content.manager_open = false;
            }
            ContentEvent::Audit(report) => self.content.audit = Some(report),
            ContentEvent::Installed(instance, message) => {
                self.content.plan = None;
                self.content.managed = None;
                self.content.managed_selection.clear();
                self.content.updates = Default::default();
                self.content.updates_checked = false;
                self.refresh(settings);
                self.selected = instance.id.clone();
                self.content.target = instance.id;
                self.notice = message;
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
