use super::*;
impl GameService {
    pub fn content_resource_instance(&mut self, id: &str, kind: crate::content::ContentKind) {
        if self.busy() {
            return;
        }
        if self.content.target != id
            || self.content.kind != kind
            || self.content.mode != content::ContentMode::Instance
        {
            self.content = ContentState::default();
            self.content.mode = content::ContentMode::Instance;
            self.content.target = id.into();
            self.content.kind = kind;
        }
        self.content.adding = true;
    }
    pub fn content_manual(&mut self, path: PathBuf, settings: &Settings, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let versions = self.content.manual_required.clone();
        self.begin(Task::Content);
        let (sender, ctx, cancel, root) = (
            self.sender.clone(),
            ctx.clone(),
            self.cancel.clone(),
            store::game_directory(settings),
        );
        let source = crate::minecraft::Source {
            paused: self.paused.clone(),
            ..Default::default()
        };
        std::thread::spawn(move || {
            let result = Downloader::new(source, cancel, reporter(&sender, &ctx)).and_then(|d| {
                let actual = crate::content::manual_file_hash(&path)?;
                let version = versions
                    .iter()
                    .find(|v| {
                        v.files
                            .iter()
                            .any(|f| f.hashes.get("sha1") == Some(&actual))
                    })
                    .ok_or("此文件不匹配任何待补齐的官方文件。")?;
                crate::content::provide_manual(&root, version, &path, &d)?;
                Ok(version.id.clone())
            });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(id) => Event::Content(content::ContentEvent::ManualReady(id)),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }
}
