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
    pub settings: Settings,
    pub save_status: String,
    pub message: String,
}

impl NsclApp {
    pub fn new() -> Self {
        let (settings, load_error) = crate::store::load();
        Self {
            page: Page::Home,
            category: 0,
            settings,
            save_status: load_error.unwrap_or_default(),
            message: String::new(),
        }
    }
}

impl eframe::App for NsclApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
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
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(RichText::new("●").size(6.0).color(theme::ACCENT));
                        ui.label(
                            RichText::new(if self.message.is_empty() {
                                "界面预览 · 核心游戏服务尚未接入"
                            } else {
                                &self.message
                            })
                            .size(10.0)
                            .color(theme::FOOTER_TEXT),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(20.0);
                            ui.label(
                                RichText::new("JAVA EDITION  ✦  NSCL")
                                    .size(10.0)
                                    .color(theme::FOOTER_TEXT),
                            );
                        });
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(theme::WINDOW_BG)
                    .inner_margin(egui::Margin {
                        left: 42,
                        right: 42,
                        top: 38,
                        bottom: 30,
                    }),
            )
            .show(ui, |ui| {
                self.page_heading(ui);
                ui.add_space(28.0);
                match self.page {
                    Page::Home => pages::home(ui, self),
                    Page::Account => pages::account(ui),
                    Page::Downloads => pages::downloads(ui, self),
                    Page::Settings => pages::settings(ui, self),
                    Page::About => pages::about(ui),
                }
            });
    }
}

impl NsclApp {
    fn page_heading(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(self.page.eyebrow())
                        .size(10.0)
                        .color(theme::EYEBROW)
                        .strong(),
                );
                ui.add_space(6.0);
                let title = if self.page == Page::Home {
                    "下一段冒险，从这里开始。"
                } else {
                    self.page.title()
                };
                ui.label(
                    RichText::new(title)
                        .size(31.0)
                        .strong()
                        .color(theme::HEADING),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                badge(ui, "开发预览");
            });
        });
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
                            egui::pos2(rect.left() + 20.0, center.y),
                            egui::pos2(rect.left() + 20.0 + brand_width, center.y),
                        ))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                brand.horizontal(|ui| {
                    ui.label(RichText::new("✦").size(29.0).color(theme::ACCENT));
                    ui.add_space(10.0);
                    if use_short_name {
                        ui.label(
                            RichText::new("NSCL")
                                .size(13.0)
                                .color(theme::BODY_TEXT)
                                .strong(),
                        );
                    } else {
                        ui.label(
                            RichText::new("NorthStar Minecraft Launcher")
                                .size(13.0)
                                .color(theme::BODY_TEXT)
                                .strong(),
                        );
                    }
                    ui.label(
                        RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .size(11.0)
                            .color(theme::MUTED),
                    );
                });

                let mut nav = ui.new_child(
                    UiBuilder::new()
                        .max_rect(egui::Rect::from_center_size(
                            center,
                            Vec2::new(480.0, rect.height()),
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
                                min_size: Vec2::new(78.0, 38.0),
                                ..Default::default()
                            },
                        );
                        if response.clicked() {
                            self.page = item;
                        }
                        if active {
                            // 原设计下划线紧贴按钮下边缘
                            let y = response.rect.bottom() - 3.0;
                            let x = response.rect.center().x;
                            ui.painter().line_segment(
                                [egui::pos2(x - 12.0, y), egui::pos2(x + 12.0, y)],
                                egui::Stroke::new(2.0, theme::ACCENT),
                            );
                        }
                    }
                });

                let mut controls = ui.new_child(
                    UiBuilder::new()
                        .max_rect(egui::Rect::from_two_pos(
                            egui::pos2(rect.right() - 140.0, center.y),
                            egui::pos2(rect.right() - 16.0, center.y),
                        ))
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                controls.horizontal(|ui| {
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
                });
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
    let radius = CornerRadius::same(style.radius as u8);
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        widgets.inactive.bg_fill = style.fill;
        widgets.inactive.corner_radius = radius;
        widgets.inactive.fg_stroke.color = theme::BODY_TEXT;
        widgets.hovered.bg_fill = style.hover;
        widgets.hovered.corner_radius = radius;
        widgets.active.bg_fill = style.active;
        widgets.active.corner_radius = radius;
        let mut button = egui::Button::new(text).min_size(style.min_size);
        if let Some(stroke) = style.stroke {
            button = button.stroke(stroke);
        }
        ui.add(button)
    })
    .inner
}

pub fn badge(ui: &mut Ui, text: &str) {
    egui::Frame::default()
        .stroke(egui::Stroke::new(1.0, theme::BADGE_BORDER))
        .corner_radius(13.0)
        .inner_margin(egui::Margin::symmetric(11, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(theme::BADGE_TEXT));
        });
}

pub fn badge_muted(ui: &mut Ui, text: &str) {
    egui::Frame::default()
        .stroke(egui::Stroke::new(1.0, theme::CARD_BORDER))
        .corner_radius(13.0)
        .inner_margin(egui::Margin::symmetric(11, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(theme::MUTED));
        });
}

pub fn primary_button(ui: &mut Ui, label: &str) -> bool {
    styled_button(
        ui,
        RichText::new(label)
            .size(14.0)
            .strong()
            .color(theme::ON_ACCENT),
        &ButtonStyle {
            fill: theme::ACCENT,
            hover: theme::ACCENT_HOVER,
            active: theme::ACCENT_HOVER,
            min_size: Vec2::new(240.0, 46.0),
            ..Default::default()
        },
    )
    .clicked()
}

pub fn secondary_button(ui: &mut Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).size(13.0).color(theme::BODY_TEXT))
            .fill(theme::SECONDARY_BG)
            .stroke(egui::Stroke::new(1.0, theme::SECONDARY_BORDER))
            .corner_radius(8.0)
            .min_size(Vec2::new(150.0, 40.0)),
    )
    .clicked()
}

pub fn eyebrow(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(10.0)
            .color(theme::EYEBROW)
            .strong(),
    );
}

pub fn heading(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(24.0)
            .strong()
            .color(theme::HEADING),
    );
}

pub fn body(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(14.0).color(theme::MUTED));
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(theme::HINT));
}
