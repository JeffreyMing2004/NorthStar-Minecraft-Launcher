use super::{Event, GameService, Settings, Task, reporter, send};
use crate::{
    content::{self, LocalAction, LocalCatalog, LocalEntry, LocalKind},
    instances::Instance,
    minecraft::Downloader,
    store,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Default)]
pub struct LocalState {
    pub key: Option<(PathBuf, String, LocalKind)>,
    pub catalog: Option<Arc<LocalCatalog>>,
    pub requested: bool,
    pub search: String,
    pub remove: Option<LocalEntry>,
    pub notice: String,
}

pub(super) enum LocalEvent {
    Listed(LocalCatalog),
    Changed(String),
    Cancelled,
}

impl GameService {
    pub fn local_list(
        &mut self,
        instance: &Instance,
        kind: LocalKind,
        settings: &Settings,
        ctx: &egui::Context,
        refresh: bool,
    ) {
        let root = store::game_directory(settings);
        let key = (root.clone(), instance.id.clone(), kind);
        if self.busy() || (!refresh && self.error.is_some()) {
            return;
        }
        if self.local.key.as_ref() != Some(&key)
            || self.local.catalog.as_ref().is_some_and(|c| {
                c.instance.version_id != instance.version_id
                    || c.instance.isolation != instance.isolation
            })
        {
            self.local = LocalState {
                key: Some(key),
                ..Default::default()
            };
        }
        if self.local.requested && !refresh {
            return;
        }
        self.local.requested = true;
        self.begin(Task::Content);
        self.progress.stage = format!("读取本地{}", kind.label());
        let (instance, sender, cancel, ctx) = (
            instance.clone(),
            self.sender.clone(),
            self.cancel.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let result = Downloader::new(Default::default(), cancel, reporter(&sender, &ctx))
                .and_then(|d| content::list_local(&root, &instance, kind, &d));
            send(
                &sender,
                &ctx,
                match result {
                    Ok(catalog) => Event::Local(LocalEvent::Listed(catalog)),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }

    pub fn local_change(
        &mut self,
        entry: LocalEntry,
        action: LocalAction,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || self.game.is_some() {
            return;
        }
        let Some(catalog) = self.local.catalog.clone() else {
            return;
        };
        self.begin(Task::Content);
        self.progress.stage = "更新本地资源".into();
        let (settings, sender, cancel, ctx) = (
            settings.clone(),
            self.sender.clone(),
            self.cancel.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let root = store::game_directory(&settings);
            let result = Downloader::new(Default::default(), cancel, reporter(&sender, &ctx))
                .and_then(|d| content::change_local(&root, &catalog, &entry, action, &d));
            send(
                &sender,
                &ctx,
                match result {
                    Ok(message) => Event::Local(LocalEvent::Changed(message)),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }

    pub fn local_import(
        &mut self,
        instance: &Instance,
        kind: LocalKind,
        paths: Option<Vec<PathBuf>>,
        settings: &Settings,
        ctx: &egui::Context,
    ) {
        if self.busy() || self.game.is_some() {
            return;
        }
        let key = (store::game_directory(settings), instance.id.clone(), kind);
        if self.local.key.as_ref() != Some(&key) {
            self.local = LocalState {
                key: Some(key),
                ..Default::default()
            };
        }
        self.local.requested = true;
        self.begin(Task::Content);
        self.progress.stage = format!("导入{}文件", kind.label());
        let (instance, settings, sender, cancel, ctx) = (
            instance.clone(),
            settings.clone(),
            self.sender.clone(),
            self.cancel.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let root = store::game_directory(&settings);
            let result = Downloader::new(Default::default(), cancel, reporter(&sender, &ctx))
                .and_then(|d| {
                    let paths = match paths {
                        Some(paths) => paths,
                        None => crate::file_picker::resources(kind)?,
                    };
                    if paths.is_empty() {
                        return Ok(None);
                    }
                    content::import_local(&root, &instance, kind, &paths, &d).map(Some)
                });
            send(
                &sender,
                &ctx,
                match result {
                    Ok(Some(message)) => Event::Local(LocalEvent::Changed(message)),
                    Ok(None) => Event::Local(LocalEvent::Cancelled),
                    Err(e) => Event::Failed(e),
                },
            );
        });
    }

    pub(super) fn apply_local(&mut self, event: LocalEvent) {
        self.task = Task::Idle;
        match event {
            LocalEvent::Listed(catalog) => self.local.catalog = Some(Arc::new(catalog)),
            LocalEvent::Cancelled => {}
            LocalEvent::Changed(message) => {
                self.local.notice = message;
                self.local.remove = None;
                self.local.requested = false;
                self.local.catalog = None;
                self.content.managed = None;
                self.content.managed_requested = false;
                self.content.managed_selection.clear();
                self.content.updates = Default::default();
                self.content.updates_checked = false;
            }
        }
    }
}
