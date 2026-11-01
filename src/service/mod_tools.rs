use super::*;
use crate::content::{self, IdentifyPlan, ModPlan};
pub enum Action {
    Identify,
    Adopt(Box<IdentifyPlan>),
    Lock(bool),
    BatchUpdates,
}
pub(super) enum ModEvent {
    Identified(Box<IdentifyPlan>),
    Changed(String),
    Plan(Box<ModPlan>),
}
impl GameService {
    pub fn mod_tools(&mut self, action: Action, settings: &Settings, ctx: &egui::Context) {
        if self.busy() || self.game.is_some() {
            return;
        }
        let target = match self.content_target(settings) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let selected = self.content.managed_selection.clone();
        let updates = self.content.updates.available.clone();
        self.begin(Task::Content);
        let (root, settings, sender, ctx, cancel) = (
            store::game_directory(settings),
            settings.clone(),
            self.sender.clone(),
            ctx.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(settings.source, cancel, reporter(&sender, &ctx))
                .and_then(|d| {
                    let api = content::Modrinth::new(d.clone());
                    match action {
                        Action::Identify => content::identify(&root, &target, &api)
                            .map(|p| ModEvent::Identified(Box::new(p))),
                        Action::Adopt(plan) => content::adopt(&root, &plan, &d).map(|_| {
                            ModEvent::Changed("本地模组来源已登记，文件及启用状态保持不变。".into())
                        }),
                        Action::Lock(locked) => content::lock_mod_versions(
                            &root, &target, &selected, locked,
                        )
                        .map(|()| {
                            ModEvent::Changed(
                                if locked {
                                    "所选版本已锁定。"
                                } else {
                                    "所选版本已解锁。"
                                }
                                .into(),
                            )
                        }),
                        Action::BatchUpdates => {
                            let mut versions = Vec::new();
                            for update in
                                updates.iter().filter(|u| selected.contains(&u.project_id))
                            {
                                versions.push(api.version(&update.available.id)?);
                            }
                            content::plan_mods(&root, target, versions, &api)
                                .map(|p| ModEvent::Plan(Box::new(p)))
                        }
                    }
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(event) => Event::ModTools(event),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }
    pub(super) fn apply_mod_tools(&mut self, event: ModEvent) {
        self.task = Task::Idle;
        match event {
            ModEvent::Identified(plan) => self.identify_plan = Some(*plan),
            ModEvent::Plan(plan) => {
                self.content.backup_before_change = true;
                self.content.manual_required = plan.manual_files.clone();
                self.content.show_manual = !self.content.manual_required.is_empty();
                self.content.plan = Some(ContentPlan::Mod(*plan));
            }
            ModEvent::Changed(message) => {
                self.identify_plan = None;
                self.content.managed = None;
                self.content.managed_requested = false;
                self.content.updates = Default::default();
                self.content.updates_checked = false;
                self.local = Default::default();
                self.notice = message;
            }
        }
    }
}
