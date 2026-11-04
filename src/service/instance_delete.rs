use super::*;

impl GameService {
    pub fn delete_instance(
        &mut self,
        instance: crate::instances::Instance,
        archive: Option<String>,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || self.game.is_some() || self.pack_import.is_some() {
            self.error = Some("请先结束当前任务或游戏，再删除实例。".into());
            return;
        }
        self.begin(Task::DeleteInstance);
        self.deleted_instance = None;
        self.progress.stage = format!("正在删除实例：{}", instance.name);
        let (root, sender, ctx) = (
            store::game_directory(settings),
            self.sender.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let result = match archive {
                Some(entry) => crate::instances::delete_archived(&root, &entry, &instance),
                None => crate::instances::delete(&root, &instance),
            };
            send(
                &sender,
                &ctx,
                match result {
                    Ok(()) => Event::InstanceDeleted(instance.id),
                    Err(error) => Event::Failed(error),
                },
            );
        });
    }
}
