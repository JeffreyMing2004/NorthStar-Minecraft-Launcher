use super::{Event, GameService, Settings, Task, reporter, send};
use crate::{
    instances::{
        Instance,
        backup::{self, CloneOptions, Scope, Snapshot},
    },
    minecraft::Downloader,
    store,
};

#[derive(Clone)]
pub enum Action {
    List,
    Create(Scope),
    Restore(String),
    Clone(CloneOptions),
}
pub(super) enum BackupEvent {
    Listed(String, Vec<Snapshot>),
    Done(String),
    Cloned(Instance),
}

impl GameService {
    pub fn backup_action(
        &mut self,
        instance: &Instance,
        action: Action,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || (self.game.is_some() && !matches!(action, Action::List)) {
            return;
        }
        if !self.instances.iter().any(|i| i == instance) {
            self.error = Some("实例已变更，请重新打开备份窗口。".into());
            return;
        }
        self.begin(Task::Content);
        self.progress.stage = "处理实例备份与克隆".into();
        let (root, destination, instance, sender, ctx, cancel) = (
            store::game_directory(settings),
            backup::directory(settings),
            instance.clone(),
            self.sender.clone(),
            ctx.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(Default::default(), cancel, reporter(&sender, &ctx))
                .and_then(|d| match action {
                    Action::List => backup::list(&destination, &instance)
                        .map(|s| BackupEvent::Listed(instance.id.clone(), s)),
                    Action::Create(scope) => {
                        backup::create(&root, &instance, &destination, scope, false, &d).map(|s| {
                            BackupEvent::Done(format!(
                                "{}备份完成（{:.1} MiB）。",
                                scope.label(),
                                s.bytes as f64 / 1048576.0
                            ))
                        })
                    }
                    Action::Restore(id) => backup::restore(&root, &instance, &destination, &id, &d)
                        .map(|()| {
                            BackupEvent::Done("恢复完成；恢复前的数据已另存为恢复点。".into())
                        }),
                    Action::Clone(options) => {
                        backup::clone_instance(&root, &instance, &options, &d)
                            .map(BackupEvent::Cloned)
                    }
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(event) => Event::Backup(event),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }
    pub(super) fn apply_backup(&mut self, event: BackupEvent, settings: &Settings) {
        self.task = Task::Idle;
        match event {
            BackupEvent::Listed(id, snapshots) => {
                self.backup_instance = id;
                self.backup_snapshots = snapshots;
            }
            BackupEvent::Done(message) => {
                self.notice = message;
                self.backup_instance.clear();
                self.refresh(settings);
                self.local = Default::default();
                self.content = Default::default();
            }
            BackupEvent::Cloned(instance) => {
                self.selected = instance.id;
                self.refresh(settings);
                self.notice = "实例已复制为新的独立实例。".into();
                self.backup_instance.clear();
            }
        }
    }
}
