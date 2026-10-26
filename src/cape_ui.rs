use crate::{account_service::AccountService, app, theme};
use egui::{RichText, TextureHandle, TextureOptions, Ui, Vec2};
use nscl::{
    accounts::{AuthClient, Session, SystemVault},
    skins::{Cape, SkinClient},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Command {
    Refresh,
    Select(Option<String>),
}

enum Event {
    Listed(Result<(Vec<Cape>, Session, String), String>),
    Texture {
        id: String,
        url: String,
        result: Result<image::RgbaImage, String>,
    },
}

struct Entry {
    cape: Cape,
    pixels: Option<image::RgbaImage>,
    texture: Option<TextureHandle>,
    error: Option<String>,
}

pub struct CapeView {
    entries: Vec<Entry>,
    account: Option<String>,
    generation: u64,
    requested: bool,
    loaded: bool,
    pub pending: bool,
    status: String,
    cancel: Arc<AtomicBool>,
    sender: Sender<(u64, Event)>,
    receiver: Receiver<(u64, Event)>,
}

impl CapeView {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            entries: Vec::new(),
            account: None,
            generation: 0,
            requested: false,
            loaded: false,
            pending: false,
            status: String::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            sender,
            receiver,
        }
    }

    fn next_generation(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation += 1;
    }

    pub fn sync(
        &mut self,
        accounts: &mut AccountService,
        online: bool,
        visible: bool,
        ctx: &egui::Context,
    ) {
        let selected = accounts
            .list
            .current()
            .filter(|a| online && a.is_online())
            .map(|a| a.id.clone());
        if selected != self.account {
            self.next_generation();
            self.account = selected;
            self.entries.clear();
            self.requested = false;
            self.loaded = false;
            self.pending = false;
            self.status.clear();
        }
        while let Ok((generation, event)) = self.receiver.try_recv() {
            if generation != self.generation {
                continue;
            }
            match event {
                Event::Listed(result) => {
                    self.pending = false;
                    match result {
                        Ok((capes, session, status))
                            if self.account.as_deref() == Some(&session.account.id) =>
                        {
                            self.set_capes(capes, status);
                            accounts.accept(session);
                        }
                        Ok(_) => self.status = "披风结果与当前账号不一致，请刷新。".into(),
                        Err(error) => self.status = error,
                    }
                }
                Event::Texture { id, url, result } => {
                    if let Some(entry) = self
                        .entries
                        .iter_mut()
                        .find(|entry| entry.cape.id == id && entry.cape.url == url)
                    {
                        match result {
                            Ok(pixels) => {
                                entry.pixels = Some(pixels);
                                entry.texture = None;
                                entry.error = None;
                            }
                            Err(error) => entry.error = Some(error),
                        }
                    }
                }
            }
        }
        if visible && self.account.is_some() && !self.requested && !accounts.pending {
            self.request(accounts, Command::Refresh, ctx);
        }
    }

    fn set_capes(&mut self, capes: Vec<Cape>, status: String) {
        let mut old = std::mem::take(&mut self.entries);
        self.entries = capes
            .into_iter()
            .map(|cape| {
                if let Some(index) = old
                    .iter()
                    .position(|entry| entry.cape.id == cape.id && entry.cape.url == cape.url)
                {
                    let mut entry = old.swap_remove(index);
                    entry.cape = cape;
                    entry
                } else {
                    Entry {
                        cape,
                        pixels: None,
                        texture: None,
                        error: None,
                    }
                }
            })
            .collect();
        self.loaded = true;
        self.status = status;
    }

    fn request(&mut self, accounts: &AccountService, command: Command, ctx: &egui::Context) {
        if self.pending || accounts.pending {
            return;
        }
        let Some((account, cached)) = accounts.launch_account() else {
            return;
        };
        if self.account.as_deref() != Some(&account.id) {
            return;
        }
        self.next_generation();
        self.pending = true;
        self.requested = true;
        self.status = match command {
            Command::Refresh => "正在加载账号披风……",
            Command::Select(_) => "正在切换披风……",
        }
        .into();
        let (generation, cancel, sender, ctx) = (
            self.generation,
            self.cancel.clone(),
            self.sender.clone(),
            ctx.clone(),
        );
        let cached_textures: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| entry.pixels.is_some())
            .map(|entry| (entry.cape.id.clone(), entry.cape.url.clone()))
            .collect();
        std::thread::spawn(move || {
            let result = (|| {
                let session = AuthClient::new()?.resume(&account, cached, &SystemVault, &cancel)?;
                if cancel.load(Ordering::Relaxed) {
                    return Err("披风操作已取消。".into());
                }
                let client = SkinClient::new()?;
                let capes = match &command {
                    Command::Refresh => client.capes(&session)?,
                    Command::Select(id) => client
                        .select_cape(&session, id.as_deref())
                        .map_err(|error| format!("{error} 请刷新列表确认当前披风。"))?,
                };
                let status = match command {
                    Command::Refresh => format!("已加载 {} 件账号披风。", capes.len()),
                    Command::Select(_) => "披风已切换，游戏内显示可能需要重新进入服务器。".into(),
                };
                Ok((client, capes, session, status))
            })();
            let (client, capes) = match result {
                Ok((client, capes, session, status)) => {
                    let _ = sender.send((
                        generation,
                        Event::Listed(Ok((capes.clone(), session, status))),
                    ));
                    ctx.request_repaint();
                    (client, capes)
                }
                Err(error) => {
                    let _ = sender.send((generation, Event::Listed(Err(error))));
                    ctx.request_repaint();
                    return;
                }
            };
            let missing: Vec<_> = capes
                .iter()
                .filter(|cape| !cached_textures.contains(&(cape.id.clone(), cape.url.clone())))
                .collect();
            // Publish the full list first; texture failures never hide an owned cape.
            for batch in missing.chunks(4) {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::scope(|scope| {
                    for cape in batch {
                        let (client, sender, ctx) = (&client, &sender, &ctx);
                        scope.spawn(move || {
                            let _ = sender.send((
                                generation,
                                Event::Texture {
                                    id: cape.id.clone(),
                                    url: cape.url.clone(),
                                    result: client.cape_texture(cape),
                                },
                            ));
                            ctx.request_repaint();
                        });
                    }
                });
            }
        });
    }

    fn rows(&mut self, ui: &mut Ui, editable: bool) -> Option<Command> {
        let mut command = None;
        if !self.loaded {
            return command;
        }
        let none_selected = !self.entries.iter().any(|entry| entry.cape.active);
        ui.push_id("no-cape", |ui| {
            theme::card(14).show(ui, |ui| {
                ui.set_width((ui.available_width() - 2.0).max(0.0));
                ui.horizontal(|ui| {
                    ui.set_min_height(36.0);
                    ui.label("无披风");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if selection_radio(ui, none_selected, editable, "无披风") {
                            command = Some(Command::Select(None));
                        }
                    });
                });
            });
        });
        ui.add_space(12.0);
        if self.entries.is_empty() {
            app::hint(ui, "当前账号没有可用披风。");
        }
        for entry in &mut self.entries {
            ui.push_id(&entry.cape.id, |ui| {
                theme::card(14).show(ui, |ui| {
                    ui.set_width((ui.available_width() - 2.0).max(0.0));
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(48.0, 76.8), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 4.0, theme::TOPBAR_BG);
                        if let Some(pixels) = &entry.pixels {
                            let texture = entry.texture.get_or_insert_with(|| {
                                ui.ctx().load_texture(
                                    format!("cape-{}", entry.cape.id),
                                    egui::ColorImage::from_rgba_unmultiplied(
                                        [pixels.width() as usize, pixels.height() as usize],
                                        pixels.as_raw(),
                                    ),
                                    TextureOptions::NEAREST,
                                )
                            });
                            ui.painter().image(
                                texture.id(),
                                rect,
                                egui::Rect::from_min_max(
                                    egui::pos2(1.0 / 64.0, 1.0 / 32.0),
                                    egui::pos2(11.0 / 64.0, 17.0 / 32.0),
                                ),
                                egui::Color32::WHITE,
                            );
                        } else {
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "披风",
                                egui::FontId::proportional(12.0),
                                theme::MUTED,
                            );
                        }
                        ui.add_space(12.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if selection_radio(ui, entry.cape.active, editable, &entry.cape.name) {
                                command = Some(Command::Select(Some(entry.cape.id.clone())));
                            }
                            ui.add_space(12.0);
                            let width = ui.available_width().max(0.0);
                            ui.allocate_ui_with_layout(
                                Vec2::new(width, 76.8),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_min_width(width);
                                    let text_height = ui.fonts_mut(|fonts| {
                                        fonts.row_height(&egui::FontId::proportional(16.0))
                                    });
                                    let has_status =
                                        entry.error.is_some() || entry.pixels.is_none();
                                    let status_height = if has_status {
                                        ui.fonts_mut(|fonts| {
                                            fonts.row_height(&egui::FontId::proportional(12.0))
                                        }) + 8.0
                                            + ui.spacing().item_spacing.y
                                    } else {
                                        0.0
                                    };
                                    ui.add_space(
                                        ((76.8 - text_height - status_height) / 2.0).max(0.0),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&entry.cape.name)
                                                .color(theme::HEADING)
                                                .size(16.0),
                                        )
                                        .truncate(),
                                    );
                                    if has_status {
                                        ui.add_space(8.0);
                                    }
                                    if let Some(error) = &entry.error {
                                        ui.label(
                                            RichText::new("预览暂不可用")
                                                .color(theme::MUTED)
                                                .size(12.0),
                                        )
                                        .on_hover_text(error);
                                    } else if entry.pixels.is_none() {
                                        app::hint(ui, "正在加载预览……");
                                    }
                                },
                            );
                        });
                    });
                });
            });
            ui.add_space(10.0);
        }
        command
    }
}

fn selection_radio(ui: &mut Ui, selected: bool, editable: bool, name: &str) -> bool {
    ui.add_enabled(editable, egui::RadioButton::new(selected, ""))
        .on_hover_text(if selected {
            format!("{name}（当前使用）")
        } else {
            format!("选择 {name}")
        })
        .clicked()
        && !selected
}

pub fn controls(ui: &mut Ui, app: &mut app::NsclApp) {
    ui.label(RichText::new("账号披风").size(18.0).color(theme::HEADING));
    ui.add_space(8.0);
    app::hint(
        ui,
        "显示当前正版账号拥有的全部披风，点击行末的圆形选择框即可使用。",
    );
    ui.add_space(16.0);
    let online =
        app.settings.online_mode && app.accounts.list.current().is_some_and(|a| a.is_online());
    let editable = online && !app.capes.pending && !app.accounts.pending && !app.skin.pending;
    if !online {
        app::body(ui, "请先选择 Microsoft 正版账号。");
        return;
    }
    let mut command = None;
    theme::toolbar(ui, |ui| {
        if ui
            .add_enabled(editable, egui::Button::new("刷新披风列表"))
            .clicked()
        {
            command = Some(Command::Refresh);
        }
        if app.capes.pending {
            ui.spinner();
        }
        ui.label(
            RichText::new(&app.capes.status)
                .color(theme::MUTED)
                .size(12.0),
        );
    });
    ui.add_space(16.0);
    command = app.capes.rows(ui, editable).or(command);
    if let Some(command) = command {
        app.capes.request(&app.accounts, command, ui.ctx());
    }
}

#[cfg(test)]
mod tests;
