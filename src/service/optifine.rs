use super::*;
impl GameService {
    pub fn optifine_import(
        &mut self,
        instance: crate::instances::Instance,
        path: PathBuf,
        forge: String,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || self.game.is_some() {
            return;
        }
        self.begin(Task::Content);
        let (root, sender, cancel, ctx, source) = (
            store::game_directory(settings),
            self.sender.clone(),
            self.cancel.clone(),
            ctx.clone(),
            settings.source.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(source, cancel, reporter(&sender, &ctx))
                .and_then(|d| crate::content::import_optifine(&root, &instance, &path, &forge, &d));
            send(
                &sender,
                &ctx,
                match result {
                    Ok(message) => Event::Local(resources::LocalEvent::Changed(message)),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }
}
