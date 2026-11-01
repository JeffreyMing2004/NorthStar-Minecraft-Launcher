use super::*;
use crate::content::{self, ContentVersion, PackOrigin, PackUpdatePlan};
use crate::instances::Instance;
#[derive(Default)]
pub struct PackState {
    pub instance: String,
    pub current: String,
    pub origin: Option<PackOrigin>,
    pub versions: Vec<ContentVersion>,
    pub plan: Option<PackUpdatePlan>,
}
pub(super) enum PackEvent {
    Loaded(Box<PackState>),
    Preview(Box<PackUpdatePlan>),
    Updated(Instance),
}
#[derive(Clone)]
pub enum Input {
    Local(PathBuf),
    Online(Box<ContentVersion>),
}
impl GameService {
    pub fn pack_load(&mut self, instance: &Instance, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        self.pack_state = PackState {
            instance: instance.id.clone(),
            ..Default::default()
        };
        self.begin(Task::Content);
        let (root, instance, settings, sender, ctx, cancel) = (
            store::game_directory(settings),
            instance.clone(),
            settings.clone(),
            self.sender.clone(),
            ctx.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = (|| {
                let record = content::registry(&instance.game_directory(&root)?)?;
                let pack = record.pack.ok_or("此实例没有整合包记录。")?;
                let versions = if let Some(origin) = &pack.origin {
                    let d = Downloader::new(settings.source, cancel, reporter(&sender, &ctx))?;
                    content::Modrinth::new(d).versions_kind(
                        &origin.project_id,
                        content::ContentKind::Modpack,
                        None,
                    )?
                } else {
                    Vec::new()
                };
                Ok(PackState {
                    instance: instance.id,
                    current: pack.version,
                    origin: pack.origin,
                    versions,
                    plan: None,
                })
            })();
            send(
                &sender,
                &ctx,
                match result {
                    Ok(s) => Event::Pack(PackEvent::Loaded(Box::new(s))),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }
    pub fn pack_preview(
        &mut self,
        instance: &Instance,
        input: Input,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || self.game.is_some() {
            return;
        }
        self.begin(Task::Content);
        let (root, instance, settings, sender, ctx, cancel) = (
            store::game_directory(settings),
            instance.clone(),
            settings.clone(),
            self.sender.clone(),
            ctx.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(settings.source, cancel, reporter(&sender, &ctx))
                .and_then(|d| {
                    let next = match input {
                        Input::Local(p) => content::inspect_pack_with_downloader(&root, &p, &d)?,
                        Input::Online(v) => {
                            content::plan_pack(&root, &v, &content::Modrinth::new(d.clone()))?
                        }
                    };
                    content::plan_update(&root, &instance, next, &d)
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(p) => Event::Pack(PackEvent::Preview(Box::new(p))),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }
    pub fn pack_apply(&mut self, settings: &Settings, ctx: &egui::Context) {
        if self.busy() || self.game.is_some() {
            return;
        }
        let Some(plan) = self.pack_state.plan.clone() else {
            return;
        };
        self.begin(Task::Content);
        let (root, settings, sender, ctx, cancel) = (
            store::game_directory(settings),
            settings.clone(),
            self.sender.clone(),
            ctx.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(settings.source.clone(), cancel, reporter(&sender, &ctx))
                .and_then(|d| content::install_update(&root, &plan, &settings, &d));
            send(
                &sender,
                &ctx,
                match result {
                    Ok(i) => Event::Pack(PackEvent::Updated(i)),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }
    pub(super) fn apply_pack(&mut self, event: PackEvent, settings: &Settings) {
        self.task = Task::Idle;
        match event {
            PackEvent::Loaded(state) => self.pack_state = *state,
            PackEvent::Preview(plan) => {
                self.content.manual_required = plan.next.manual_files.clone();
                self.content.show_manual = !self.content.manual_required.is_empty();
                self.pack_state.plan = Some(*plan);
            }
            PackEvent::Updated(instance) => {
                self.selected = instance.id;
                self.refresh(settings);
                self.content = Default::default();
                self.local = Default::default();
                self.pack_state = Default::default();
                self.notice = "整合包版本已更新；原数据可从备份与复制中的恢复点回退。".into();
            }
        }
    }
}
