use super::*;
impl GameService {
    pub fn copy_migration(
        &mut self,
        plan: crate::instances::migration::Plan,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || self.game.is_some() {
            return;
        }
        let root = store::game_directory(settings);
        if !crate::instances::migration::target_matches(&plan, &root) {
            self.error = Some("目标游戏目录已改变，请重新预览迁入方案。".into());
            return;
        }
        self.begin(Task::Content);
        let (sender, cancel, ctx, source) = (
            self.sender.clone(),
            self.cancel.clone(),
            ctx.clone(),
            settings.source.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(source, cancel, reporter(&sender, &ctx))
                .and_then(|d| crate::instances::migration::copy(&plan, &d));
            send(
                &sender,
                &ctx,
                match result {
                    Ok(i) => Event::Migrated(i),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }
}
