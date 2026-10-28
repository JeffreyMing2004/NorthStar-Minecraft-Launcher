use super::{Event, GameService, Settings, Task, reporter, send};
use crate::{
    content::{self, PackExportOptions},
    instances::Instance,
    minecraft::Downloader,
    store,
};
use std::path::PathBuf;

impl GameService {
    pub fn export_pack(
        &mut self,
        instance: &Instance,
        options: PackExportOptions,
        destination: PathBuf,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || self.game.is_some() {
            return;
        }
        if !self.instances.iter().any(|i| i == instance) {
            self.error = Some("实例已变更，请重新打开导出窗口。".into());
            return;
        }
        if let Err(error) = options.validate() {
            self.error = Some(error);
            return;
        }
        self.begin(Task::Content);
        self.exported_pack = None;
        self.progress.stage = "准备导出整合包".into();
        let (root, instance, ctx, sender, cancel) = (
            store::game_directory(settings),
            instance.clone(),
            ctx.clone(),
            self.sender.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(Default::default(), cancel, reporter(&sender, &ctx))
                .and_then(|d| content::export_pack(&root, &instance, &options, &destination, &d));
            send(
                &sender,
                &ctx,
                match result {
                    Ok(path) => Event::PackExported(path),
                    Err(error) => Event::Failed(error),
                },
            );
        });
    }
}
