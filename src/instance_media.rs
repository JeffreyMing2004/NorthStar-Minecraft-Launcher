use crate::{app::NsclApp, instances::Instance};
use nscl::{
    game_data::{self, Catalog, Screenshot},
    minecraft::Downloader,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
};

type ImageResult = Result<image::RgbaImage, String>;
#[derive(Default)]
pub struct Browser {
    key: Option<(PathBuf, String)>,
    catalog: Option<Catalog>,
    pending: Option<Receiver<Result<Catalog, String>>>,
    cancel: Arc<AtomicBool>,
    images: BTreeMap<String, (u64, Option<egui::TextureHandle>)>,
    loading: Vec<(String, Receiver<ImageResult>)>,
    clock: u64,
    query: String,
    recent: bool,
    error: String,
}
impl Drop for Browser {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Browser {
    fn request(&mut self, root: &std::path::Path, instance: &Instance, ctx: &egui::Context) {
        let key = (
            instance
                .game_directory(root)
                .unwrap_or_else(|_| root.to_path_buf()),
            instance.id.clone(),
        );
        if self.key.as_ref() != Some(&key) {
            let mut next = Self::default();
            next.key = Some(key);
            *self = next;
        }
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(Ok(catalog)) => {
                    self.catalog = Some(catalog);
                    self.pending = None;
                }
                Ok(Err(e)) => {
                    self.error = e;
                    self.pending = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.error = "读取任务提前结束，请刷新重试。".into();
                    self.pending = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.catalog.is_none() && self.pending.is_none() && self.error.is_empty() {
            let (root, instance, cancel, ctx) = (
                root.to_path_buf(),
                instance.clone(),
                self.cancel.clone(),
                ctx.clone(),
            );
            let (tx, rx) = mpsc::channel();
            self.pending = Some(rx);
            std::thread::spawn(move || {
                let result = Downloader::new(Default::default(), cancel, Arc::new(|_| {}))
                    .and_then(|d| game_data::list(&root, &instance, &d));
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        }
    }
    fn image(
        &mut self,
        directory: &std::path::Path,
        screenshot: &Screenshot,
        ctx: &egui::Context,
    ) -> Option<egui::TextureHandle> {
        self.loading.retain(|(name, rx)| match rx.try_recv() {
            Ok(result) => {
                let texture = result.ok().map(|image| {
                    ctx.load_texture(
                        format!("screenshot:{name}"),
                        egui::ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        ),
                        egui::TextureOptions::LINEAR,
                    )
                });
                self.images.insert(name.clone(), (self.clock, texture));
                false
            }
            Err(TryRecvError::Disconnected) => {
                self.images.insert(name.clone(), (self.clock, None));
                false
            }
            Err(TryRecvError::Empty) => true,
        });
        self.clock += 1;
        if let Some((used, texture)) = self.images.get_mut(&screenshot.file) {
            *used = self.clock;
            return texture.clone();
        }
        while self.images.len() >= 64 {
            let key = self
                .images
                .iter()
                .min_by_key(|(_, v)| v.0)
                .unwrap()
                .0
                .clone();
            self.images.remove(&key);
        }
        if self.loading.len() < 2
            && !self
                .loading
                .iter()
                .any(|(name, _)| name == &screenshot.file)
        {
            let (directory, screenshot, ctx) =
                (directory.to_path_buf(), screenshot.clone(), ctx.clone());
            let (tx, rx) = mpsc::channel();
            self.loading.push((screenshot.file.clone(), rx));
            std::thread::spawn(move || {
                let _ = tx.send(game_data::thumbnail(&directory, &screenshot));
                ctx.request_repaint();
            });
        }
        None
    }
}
fn age(seconds: u64) -> String {
    if seconds == 0 {
        return "时间未知".into();
    }
    let elapsed = nscl::accounts::now().saturating_sub(seconds);
    if elapsed < 3600 {
        format!("{} 分钟前", elapsed / 60)
    } else if elapsed < 86400 {
        format!("{} 小时前", elapsed / 3600)
    } else {
        format!("{} 天前", elapsed / 86400)
    }
}
pub fn show(ui: &mut egui::Ui, app: &mut NsclApp, instance: &Instance, screenshots: bool) {
    let root = nscl::store::game_directory(&app.settings);
    let mut browser = std::mem::take(&mut app.instance_browser.media);
    browser.request(&root, instance, ui.ctx());
    let directory = match instance.game_directory(&root) {
        Ok(p) => p,
        Err(e) => {
            ui.label(e);
            return;
        }
    };
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut browser.query)
                .hint_text(if screenshots {
                    "筛选截图文件名"
                } else {
                    "搜索世界名称、文件夹或版本"
                })
                .desired_width(180.0),
        );
        if ui.button("刷新").clicked() {
            browser.catalog = None;
            browser.images.clear();
            browser.loading.clear();
            browser.error.clear();
        }
        if screenshots {
            ui.checkbox(&mut browser.recent, "最近 7 天");
        } else if ui
            .add_enabled(
                !app.service.busy() && app.service.game.is_none(),
                egui::Button::new("备份所有存档"),
            )
            .clicked()
        {
            app.service.backup_action(
                instance,
                crate::service::backup::Action::Create(nscl::instances::backup::Scope::Saves),
                &app.settings,
                ui.ctx(),
            );
        }
    });
    if browser.pending.is_some() {
        ui.spinner();
    }
    if !browser.error.is_empty() {
        ui.label(&browser.error);
    }
    let mut open = None;
    if let Some(catalog) = &browser.catalog {
        if catalog.truncated {
            ui.label("目录过大，仅显示前 1000 个条目。");
        }
        let query = browser.query.to_lowercase();
        if screenshots {
            let entries: Vec<_> = catalog
                .screenshots
                .iter()
                .filter(|s| {
                    s.file.to_lowercase().contains(&query)
                        && (!browser.recent
                            || nscl::accounts::now().saturating_sub(s.modified) < 7 * 86400)
                })
                .cloned()
                .collect();
            if entries.is_empty() {
                ui.label("没有匹配的截图。");
            }
            let height = (ui.clip_rect().bottom() - ui.cursor().top() - 10.0).max(100.0);
            egui::ScrollArea::vertical().max_height(height).show_rows(
                ui,
                140.0,
                entries.len(),
                |ui, range| {
                    for index in range {
                        let item = &entries[index];
                        ui.horizontal(|ui| {
                            if let Some(texture) = browser.image(&directory, item, ui.ctx()) {
                                if ui
                                    .add(egui::Button::image(
                                        egui::Image::new(&texture)
                                            .fit_to_exact_size(egui::vec2(180.0, 100.0)),
                                    ))
                                    .clicked()
                                {
                                    open = Some(format!("screenshots/{}", item.file));
                                }
                            } else {
                                ui.allocate_space(egui::vec2(180.0, 100.0));
                            }
                            ui.vertical(|ui| {
                                ui.add(egui::Label::new(&item.file).truncate())
                                    .on_hover_text(&item.file);
                                ui.label(format!(
                                    "{} · {:.1} MiB",
                                    age(item.modified),
                                    item.size as f64 / 1048576.0
                                ));
                                if ui.button("打开原图").clicked() {
                                    open = Some(format!("screenshots/{}", item.file));
                                }
                            });
                        });
                    }
                },
            );
        } else {
            let entries: Vec<_> = catalog
                .worlds
                .iter()
                .filter(|w| {
                    format!("{} {} {}", w.name, w.folder, w.version)
                        .to_lowercase()
                        .contains(&query)
                })
                .collect();
            if entries.is_empty() {
                ui.label("此实例没有匹配的存档。");
            }
            let height = (ui.clip_rect().bottom() - ui.cursor().top() - 10.0).max(100.0);
            egui::ScrollArea::vertical()
                .max_height(height)
                .show(ui, |ui| {
                    for world in entries {
                        ui.group(|ui| {
                            ui.strong(&world.name);
                            ui.label(format!(
                                "{} · {} · {}",
                                world.version,
                                world.game_mode,
                                age(world.last_played)
                            ));
                            ui.label(format!("文件夹：{}", world.folder));
                            if let Some(warning) = &world.warning {
                                ui.label(warning);
                            }
                            if ui.button("打开世界目录").clicked() {
                                open = Some(format!("saves/{}", world.folder));
                            }
                        });
                    }
                });
        }
    }
    if let Some(relative) = open
        && let Err(e) = game_data::checked_path(&directory, &relative)
            .and_then(|p| open::that_detached(p).map_err(|e| e.to_string()))
    {
        browser.error = e;
    }
    app.instance_browser.media = browser;
}
