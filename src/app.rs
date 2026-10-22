use crate::{pages, store::Settings, theme};
use egui::{Color32, Context, CornerRadius, RichText, Sense, Ui, UiBuilder, Vec2, ViewportCommand};

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Home,
    Account,
    Downloads,
    Settings,
    About,
}

impl Page {
    pub const ALL: [Page; 5] = [
        Page::Home,
        Page::Account,
        Page::Downloads,
        Page::Settings,
        Page::About,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Page::Home => "开始",
            Page::Account => "账号",
            Page::Downloads => "下载",
            Page::Settings => "设置",
            Page::About => "更多",
        }
    }
    fn eyebrow(self) -> &'static str {
        match self {
            Page::Home => "NORTHSTAR / HOME",
            Page::Account => "NORTHSTAR / ACCOUNT",
            Page::Downloads => "NORTHSTAR / DOWNLOADS",
            Page::Settings => "NORTHSTAR / SETTINGS",
            Page::About => "NORTHSTAR / MORE",
        }
    }
}

pub struct NsclApp {
    pub page: Page,
    pub category: usize,
    pub download: crate::download_flow::DownloadFlow,
    pub settings: Settings,
    pub memory_input: String,
    pub save_status: String,
    pub service: crate::service::GameService,
    pub project_icons: crate::project_icons::ProjectIcons,
    pub log_view: Option<String>,
    pub accounts: crate::account_service::AccountService,
    pub instance_manager: bool,
    pub instance_edit: Option<crate::instances::Instance>,
    pub instance_memory: String,
    pub new_instance_name: String,
    pub new_instance_version: String,
    pub archive_confirm: Option<crate::instances::Instance>,
    pub instance_status: String,
    #[cfg(debug_assertions)]
    pub screenshot_pending: bool,
}

impl NsclApp {
    pub fn new() -> Self {
        let (settings, load_error) = crate::store::load();
        #[cfg(debug_assertions)]
        let settings = {
            let mut settings = settings;
            if let Some(path) = std::env::var_os("NSCL_PREVIEW_GAME_DIR") {
                settings.game_directory = path.to_string_lossy().into_owned();
            }
            settings
        };
        let service = crate::service::GameService::new(&settings);
        Self {
            page: initial_page(),
            category: 0,
            download: crate::download_flow::DownloadFlow::default(),
            memory_input: settings.memory.to_string(),
            settings,
            save_status: load_error.unwrap_or_default(),
            service,
            project_icons: crate::project_icons::ProjectIcons::new(
                crate::store::settings_path()
                    .with_file_name("cache")
                    .join("project-icons"),
            ),
            log_view: None,
            accounts: crate::account_service::AccountService::new(),
            instance_manager: false,
            instance_edit: None,
            instance_memory: String::new(),
            new_instance_name: String::new(),
            new_instance_version: String::new(),
            archive_confirm: None,
            instance_status: String::new(),
            #[cfg(debug_assertions)]
            screenshot_pending: std::env::var_os("NSCL_SCREENSHOT").is_some(),
        }
    }
}

impl eframe::App for NsclApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(debug_assertions)]
        if std::env::var_os("NSCL_SCREENSHOT").is_some()
            && ctx.input(|input| input.key_pressed(egui::Key::F12))
        {
            self.screenshot_pending = true;
        }
        let previous_instance = self.service.selected.clone();
        self.service.poll(&self.settings);
        if previous_instance != self.service.selected {
            self.remember_instance();
        }
        self.accounts.poll(&mut self.settings, &ctx);
        if let Some(session) = self.service.renewed_session.take() {
            self.accounts.accept(session);
        }
        if let Some(error) = self.accounts.error.take() {
            self.service.error = Some(error);
        }
        pages::instance_windows(&ctx, self);
        if let Some(text) = &self.log_view {
            let mut open = true;
            let mut refresh = false;
            egui::Window::new("游戏启动日志")
                .open(&mut open)
                .default_width(800.0)
                .show(&ctx, |ui| {
                    if ui.button("刷新日志").clicked() {
                        refresh = true;
                    }
                    egui::ScrollArea::both().max_height(450.0).show(ui, |ui| {
                        ui.label(RichText::new(text).monospace());
                    });
                });
            if !open {
                self.log_view = None;
            } else if refresh {
                self.open_log();
            }
        }
        if let Some(error) = self.service.error.clone() {
            let mut open = true;
            egui::Window::new("操作未完成")
                .open(&mut open)
                .default_width(600.0)
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            ui.label(&error);
                        });
                    if let Some(path) = &self.service.last_log {
                        ui.label(format!("启动日志：{}", path.display()));
                    }
                    if ui.button("复制错误信息").clicked() {
                        ctx.copy_text(error);
                    }
                });
            if !open {
                self.service.error = None;
            }
        }
        #[cfg(debug_assertions)]
        if self.screenshot_pending
            && (self.page != Page::Downloads
                || !self.service.versions.is_empty()
                || self.service.error.is_some()
                || !self.service.notice.is_empty())
        {
            self.screenshot_pending = false;
            ctx.request_screenshot(move |image| save_screenshot(&image));
        }
        self.top_bar(ui, &ctx);
        egui::Panel::bottom("footer")
            .exact_size(42.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::default()
                    .fill(theme::TOPBAR_BG)
                    .stroke(egui::Stroke::new(1.0, theme::CARD_BORDER)),
            )
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    ui.available_size(),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add_space(20.0);
                        ui.label(RichText::new("●").size(6.0).color(theme::ACCENT));
                        ui.label(
                            RichText::new(if self.service.busy() {
                                self.service.progress.stage.as_str()
                            } else if self.service.game.is_some() {
                                "游戏正在运行"
                            } else {
                                "启动器 · 准备就绪"
                            })
                            .size(10.0)
                            .color(theme::FOOTER_TEXT),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(20.0);
                            ui.label(RichText::new("NSCL").size(10.0).color(theme::FOOTER_TEXT));
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::new(16.0, 12.0), egui::Sense::hover());
                            theme::star4(ui.painter(), rect.center(), 5.0, theme::FOOTER_TEXT);
                            ui.label(
                                RichText::new("JAVA EDITION")
                                    .size(10.0)
                                    .color(theme::FOOTER_TEXT),
                            );
                        });
                    },
                );
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(theme::WINDOW_BG)
                    .inner_margin(egui::Margin {
                        left: 42,
                        right: 42,
                        top: if self.page == Page::Downloads { 28 } else { 38 },
                        bottom: if self.page == Page::Downloads { 24 } else { 30 },
                    }),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                self.page_heading(ui);
                if self.page == Page::Downloads {
                    ui.add_space(20.0);
                    pages::downloads(ui, self);
                    return;
                }
                ui.add_space(28.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.page {
                        Page::Home => pages::home(ui, self),
                        Page::Account => pages::account(ui, self),
                        Page::Downloads => unreachable!(),
                        Page::Settings => pages::settings(ui, self),
                        Page::About => pages::about(ui),
                    });
            });
    }
}

impl NsclApp {
    pub fn remember_instance(&mut self) {
        self.settings.selected_instance = self.service.selected.clone();
        let (mut saved, _) = crate::store::load();
        saved.selected_instance = self.service.selected.clone();
        if let Err(error) = crate::store::save(&saved) {
            self.service.error = Some(error);
        }
    }
    pub fn open_log(&mut self) {
        use std::io::{Read, Seek, SeekFrom};
        let Some(path) = &self.service.last_log else {
            return;
        };
        let result = (|| -> std::io::Result<String> {
            let mut file = std::fs::File::open(path)?;
            let length = file.metadata()?.len();
            file.seek(SeekFrom::Start(length.saturating_sub(65536)))?;
            let mut bytes = Vec::new();
            file.take(65536).read_to_end(&mut bytes)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        })();
        match result {
            Ok(text) => self.log_view = Some(text),
            Err(error) => self.service.error = Some(format!("读取日志失败：{error}")),
        }
    }

    fn page_heading(&mut self, ui: &mut Ui) {
        let height = if self.page == Page::Downloads {
            64.0
        } else {
            68.0
        };
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        theme::spaced_text(
            ui.painter(),
            rect.left_top(),
            self.page.eyebrow(),
            10.0,
            2.5,
            theme::EYEBROW,
        );
        let title = if self.page == Page::Home {
            "下一段冒险，从这里开始。"
        } else {
            self.page.title()
        };
        ui.painter().text(
            rect.left_top() + Vec2::new(0.0, 27.0),
            egui::Align2::LEFT_TOP,
            title,
            egui::FontId::new(28.0, theme::bold_family()),
            theme::HEADING,
        );
        paint_badge(
            ui.painter(),
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - 68.0, rect.center().y - 13.0),
                Vec2::new(68.0, 26.0),
            ),
            "开发预览",
            false,
        );
    }

    fn top_bar(&mut self, ui: &mut Ui, ctx: &Context) {
        egui::Panel::top("topbar")
            .exact_size(72.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::default()
                    .fill(theme::TOPBAR_BG)
                    .stroke(egui::Stroke::new(1.0, theme::CARD_BORDER)),
            )
            .show(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                let center = rect.center();
                let drag = ui.interact(
                    rect,
                    ui.scope_id().with(egui::IdSalt::new("topbar-drag")),
                    Sense::drag(),
                );
                if drag.dragged() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }

                let use_short_name = rect.width() < 1320.0;
                let brand_width = if use_short_name { 140.0 } else { 340.0 };
                let mut brand = ui.new_child(
                    UiBuilder::new()
                        .max_rect(egui::Rect::from_two_pos(
                            egui::pos2(rect.left() + 20.0, rect.top()),
                            egui::pos2(rect.left() + 20.0 + brand_width, rect.bottom()),
                        ))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                brand.allocate_ui_with_layout(
                    brand.available_size(),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let (star_rect, _) =
                            ui.allocate_exact_size(Vec2::new(26.0, 26.0), egui::Sense::hover());
                        theme::star4(ui.painter(), star_rect.center(), 13.0, theme::ACCENT);
                        ui.add_space(10.0);
                        if use_short_name {
                            ui.label(
                                RichText::new("NSCL")
                                    .size(13.0)
                                    .color(theme::BODY_TEXT)
                                    .family(theme::bold_family()),
                            );
                        } else {
                            ui.label(
                                RichText::new("NorthStar Minecraft Launcher")
                                    .size(13.0)
                                    .color(theme::BODY_TEXT)
                                    .family(theme::bold_family()),
                            );
                        }
                        ui.label(
                            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(11.0)
                                .color(theme::MUTED),
                        );
                    },
                );

                // 与原设计一致：五个 64px 按钮与四个 6px 间隙，整组相对窗口居中。
                let mut nav = ui.new_child(
                    UiBuilder::new()
                        .max_rect(egui::Rect::from_center_size(
                            center,
                            Vec2::new(344.0, rect.height()),
                        ))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                nav.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                    for item in Page::ALL {
                        let active = self.page == item;
                        let fill = if active {
                            theme::nav_active_bg()
                        } else {
                            Color32::TRANSPARENT
                        };
                        let text = RichText::new(item.title()).size(14.0).color(if active {
                            theme::NAV_ACTIVE
                        } else {
                            theme::NAV_TEXT
                        });
                        let response = styled_button(
                            ui,
                            text,
                            &ButtonStyle {
                                fill,
                                radius: 7.0,
                                min_size: Vec2::new(64.0, 38.0),
                                ..Default::default()
                            },
                        );
                        if response.clicked() {
                            self.page = item;
                        }
                        if active {
                            // 原设计下划线悬于按钮下方数像素处
                            let y = rect.bottom() - 1.0;
                            let x = response.rect.center().x;
                            ui.painter().line_segment(
                                [egui::pos2(x - 10.0, y), egui::pos2(x + 10.0, y)],
                                egui::Stroke::new(2.0, theme::ACCENT),
                            );
                        }
                    }
                });

                let mut controls = ui.new_child(
                    UiBuilder::new()
                        .max_rect(egui::Rect::from_two_pos(
                            egui::pos2(rect.right() - 140.0, rect.top()),
                            egui::pos2(rect.right() - 16.0, rect.bottom()),
                        ))
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                controls.allocate_ui_with_layout(
                    controls.available_size(),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        window_button(ui, "✕", egui::Color32::from_rgb(0xb7, 0x39, 0x49), || {
                            ctx.send_viewport_cmd(ViewportCommand::Close)
                        });
                        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        window_button(ui, "▢", theme::HOVER, || {
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized))
                        });
                        window_button(ui, "—", theme::HOVER, || {
                            ctx.send_viewport_cmd(ViewportCommand::Minimized(true))
                        });
                    },
                );
            });
    }
}

fn window_button(ui: &mut Ui, label: &str, hover: Color32, action: impl FnOnce()) {
    let response = styled_button(
        ui,
        RichText::new(label).size(14.0).color(theme::BODY_TEXT),
        &ButtonStyle {
            hover,
            active: hover,
            radius: 5.0,
            min_size: Vec2::new(34.0, 30.0),
            ..Default::default()
        },
    );
    if response.clicked() {
        action();
    }
}

pub struct ButtonStyle {
    pub fill: Color32,
    pub hover: Color32,
    pub active: Color32,
    pub radius: f32,
    pub min_size: Vec2,
    pub stroke: Option<egui::Stroke>,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            fill: Color32::TRANSPARENT,
            hover: theme::HOVER,
            active: theme::HOVER,
            radius: 8.0,
            min_size: Vec2::ZERO,
            stroke: None,
        }
    }
}

pub fn styled_button(ui: &mut Ui, text: RichText, style: &ButtonStyle) -> egui::Response {
    let label = text.text().to_owned();
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let size = (galley.size() + Vec2::new(24.0, 16.0)).max(style.min_size);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label));
    let fill = if response.is_pointer_button_down_on() {
        style.active
    } else if response.hovered() {
        style.hover
    } else {
        style.fill
    };
    let mut painter = ui.painter().clone();
    if !ui.is_enabled() {
        painter.multiply_opacity(0.5);
    }
    painter.rect_filled(rect, CornerRadius::same(style.radius as u8), fill);
    if let Some(stroke) = style.stroke {
        painter.rect_stroke(rect, style.radius, stroke, egui::StrokeKind::Inside);
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(3.0),
            style.radius,
            egui::Stroke::new(2.0, theme::ACCENT),
            egui::StrokeKind::Outside,
        );
    }
    painter.galley(
        rect.center() - galley.size() * 0.5,
        galley,
        theme::BODY_TEXT,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn badge_muted(ui: &mut Ui, text: &str) {
    badge_widget(ui, text, true);
}

fn badge_widget(ui: &mut Ui, text: &str, muted: bool) {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(11.0),
        theme::BODY_TEXT,
    );
    let (rect, _) = ui.allocate_exact_size(Vec2::new(galley.size().x + 22.0, 26.0), Sense::hover());
    paint_badge(ui.painter(), rect, text, muted);
}

fn paint_badge(painter: &egui::Painter, rect: egui::Rect, text: &str, muted: bool) {
    painter.rect_stroke(
        rect,
        13.0,
        egui::Stroke::new(
            1.0,
            if muted {
                theme::CARD_BORDER
            } else {
                theme::BADGE_BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(11.0),
        if muted {
            theme::MUTED
        } else {
            theme::BADGE_TEXT
        },
    );
}

pub fn primary_button(ui: &mut Ui, label: &str) -> bool {
    styled_button(
        ui,
        RichText::new(label)
            .size(14.0)
            .family(theme::bold_family())
            .color(theme::ON_ACCENT),
        &ButtonStyle {
            fill: theme::ACCENT,
            hover: theme::ACCENT_HOVER,
            active: theme::ACCENT_HOVER,
            min_size: Vec2::new(
                if label == "保存设置" {
                    100.0
                } else {
                    200.0
                },
                46.0,
            ),
            ..Default::default()
        },
    )
    .clicked()
}

pub fn eyebrow(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    theme::spaced_text(
        ui.painter(),
        rect.left_top(),
        text,
        10.0,
        2.5,
        theme::EYEBROW,
    );
}

pub fn heading(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(21.0)
            .family(theme::bold_family())
            .color(theme::HEADING),
    );
}

pub fn body(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(14.0).color(theme::MUTED));
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(theme::HINT));
}

fn initial_page() -> Page {
    #[cfg(debug_assertions)]
    if let Ok(title) = std::env::var("NSCL_PREVIEW_PAGE") {
        return Page::ALL
            .into_iter()
            .find(|page| page.title() == title)
            .unwrap_or(Page::Home);
    }
    Page::Home
}

// 显式启用的开发截图，用于与 image/ 中的参照图核对；发布构建不包含此功能。
#[cfg(debug_assertions)]
fn save_screenshot(image: &egui::ColorImage) {
    use std::io::Write;
    let (width, height) = (image.size[0] as u32, image.size[1] as u32);
    let row = (width * 3) as usize;
    let padding = (4 - row % 4) % 4;
    let data_size = (row + padding) * height as usize;
    let mut header = Vec::with_capacity(54);
    header.extend_from_slice(b"BM");
    header.extend_from_slice(&((54 + data_size) as u32).to_le_bytes());
    header.extend_from_slice(&[0; 4]);
    header.extend_from_slice(&54u32.to_le_bytes());
    header.extend_from_slice(&40u32.to_le_bytes());
    header.extend_from_slice(&(width as i32).to_le_bytes());
    header.extend_from_slice(&(height as i32).to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&24u16.to_le_bytes());
    header.extend_from_slice(&[0; 4]);
    header.extend_from_slice(&(data_size as u32).to_le_bytes());
    header.extend_from_slice(&[0; 16]);
    let mut body = Vec::with_capacity(data_size);
    for y in (0..height).rev() {
        for x in 0..width {
            let pixel = image.pixels[y as usize * width as usize + x as usize];
            body.extend_from_slice(&[pixel.b(), pixel.g(), pixel.r()]);
        }
        body.extend(std::iter::repeat_n(0u8, padding));
    }
    let path = std::env::var("NSCL_SCREENSHOT")
        .ok()
        .filter(|path| path != "1")
        .unwrap_or_else(|| "target/debug/nscl-shot.bmp".to_owned());
    let Ok(mut file) = std::fs::File::create(path) else {
        return;
    };
    let _ = file.write_all(&header);
    let _ = file.write_all(&body);
}
