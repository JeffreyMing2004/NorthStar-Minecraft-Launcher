use crate::{pages, store::Settings, theme};
use egui::{Color32, Context, CornerRadius, RichText, Sense, Ui, UiBuilder, Vec2, ViewportCommand};

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Home,
    Instances,
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
            Page::Instances => "实例管理",
            Page::Account => "账号",
            Page::Downloads => "下载",
            Page::Settings => "设置",
            Page::About => "更多",
        }
    }
    fn icon(self) -> theme::Icon {
        match self {
            Page::Home => theme::Icon::Play,
            Page::Instances => theme::Icon::Folder,
            Page::Account => theme::Icon::User,
            Page::Downloads => theme::Icon::Download,
            Page::Settings => theme::Icon::Settings,
            Page::About => theme::Icon::More,
        }
    }
}

pub struct NsclApp {
    pub page: Page,
    pub category: usize,
    pub account_tab: u8,
    pub offline_name: String,
    pub new_game_directory: String,
    pub download: crate::download_flow::DownloadFlow,
    pub settings: Settings,
    pub memory_input: String,
    pub save_status: String,
    pub service: crate::service::GameService,
    pub project_icons: crate::project_icons::ProjectIcons,
    pub skin: crate::skin_ui::SkinView,
    pub capes: crate::cape_ui::CapeView,
    pub updates: crate::update_service::UpdateService,
    pub log_view: Option<String>,
    pub accounts: crate::account_service::AccountService,
    pub instance_browser: crate::instance_ui::BrowserState,
    pub instance_icons: crate::instance_icons::InstanceIcons,
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
        let mut settings = settings;
        let accounts = crate::account_service::AccountService::new();
        settings.online_mode = accounts.list.current().is_some_and(|a| a.is_online());
        let service = crate::service::GameService::new(&settings);
        Self {
            page: initial_page(),
            category: 0,
            account_tab: 0,
            offline_name: String::new(),
            new_game_directory: String::new(),
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
            skin: crate::skin_ui::SkinView::new(),
            capes: crate::cape_ui::CapeView::new(),
            updates: crate::update_service::UpdateService::new(),
            log_view: None,
            accounts,
            instance_browser: crate::instance_ui::BrowserState::default(),
            instance_icons: crate::instance_icons::InstanceIcons::default(),
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
        self.updates.poll();
        if self.settings.check_updates && !self.updates.started {
            self.updates.check(&ctx);
        }
        self.handle_pack_drop(&ctx);
        self.handle_resource_drop(&ctx);
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
        self.skin
            .sync(&mut self.accounts, self.settings.online_mode, &ctx);
        self.capes.sync(
            &mut self.accounts,
            self.settings.online_mode,
            self.page == Page::Account && self.account_tab == 2,
            &ctx,
        );
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
        crate::error_ui::show(&ctx, self);
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
            .exact_size(32.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::default()
                    .fill(theme::WINDOW_BG)
                    .stroke(egui::Stroke::new(1.0, theme::CARD_BORDER)),
            )
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    ui.available_size(),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add_space(28.0);
                        ui.label(RichText::new("●").size(6.0).color(theme::ACCENT));
                        ui.label(
                            RichText::new(if self.service.busy() {
                                self.service.progress.stage.as_str()
                            } else if self.service.game.is_some() {
                                "游戏正在运行"
                            } else if self.updates.available() {
                                "发现启动器新版本，请前往「更多」更新"
                            } else {
                                "准备就绪"
                            })
                            .size(11.0)
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
                        left: 32,
                        right: 32,
                        top: 26,
                        bottom: 24,
                    }),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if self.page == Page::Downloads {
                    pages::downloads(ui, self);
                    return;
                }
                if self.page == Page::Settings {
                    pages::settings(ui, self);
                    return;
                }
                if self.page == Page::Instances {
                    crate::instance_ui::show(ui, self);
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt(("page-content", self.page.title()))
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.page {
                        Page::Home => pages::home(ui, self),
                        Page::Instances => unreachable!(),
                        Page::Account => pages::account(ui, self),
                        Page::Downloads => unreachable!(),
                        Page::Settings => pages::settings(ui, self),
                        Page::About => pages::about(ui, self),
                    });
            });
        crate::renderer::record_first_frame(&ctx);
    }
}

impl NsclApp {
    pub(crate) fn handle_resource_drop(&mut self, ctx: &egui::Context) {
        if self.page != Page::Instances {
            return;
        }
        let kind = match self.instance_browser.tab {
            crate::instance_ui::DetailTab::Mods => crate::content::LocalKind::Mods,
            crate::instance_ui::DetailTab::ResourcePacks => {
                crate::content::LocalKind::ResourcePacks
            }
            crate::instance_ui::DetailTab::Shaders => crate::content::LocalKind::Shaders,
            _ => return,
        };
        let paths: Vec<_> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if paths.is_empty() {
            return;
        }
        ctx.input_mut(|i| i.raw.dropped_files.clear());
        if self.instance_browser.filter == crate::instance_ui::Filter::Archived {
            self.service.error = Some("请先恢复此实例，再导入资源。".into());
            return;
        }
        if self.service.busy() || self.service.game.is_some() {
            self.service.error = Some("请先结束当前任务并退出游戏，再导入资源。".into());
            return;
        }
        let Some(instance) = self
            .service
            .instances
            .iter()
            .find(|i| i.id == self.instance_browser.selected)
            .cloned()
        else {
            return;
        };
        if kind == crate::content::LocalKind::Mods
            && !self
                .service
                .installed
                .iter()
                .any(|v| v.id == instance.version_id && v.loader.is_some())
        {
            self.service.error = Some("此实例未安装模组加载器。".into());
            return;
        }
        self.service.content.adding = false;
        self.service.content.project = None;
        self.service.content.plan = None;
        self.service
            .local_import(&instance, kind, Some(paths), &self.settings, ctx);
    }

    pub(crate) fn handle_pack_drop(&mut self, ctx: &egui::Context) {
        if !matches!(self.page, Page::Home | Page::Downloads) {
            return;
        }
        let files: Vec<_> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| {
                    p.extension().is_some_and(|e| {
                        e.eq_ignore_ascii_case("mrpack") || e.eq_ignore_ascii_case("zip")
                    })
                })
                .collect()
        });
        if files.is_empty() {
            return;
        }
        // A drop belongs to one task, even if egui requests another layout pass.
        ctx.input_mut(|i| i.raw.dropped_files.clear());
        if files.len() != 1 {
            self.service.error = Some("请每次拖入一个整合包。".into());
            return;
        }
        if self.service.busy() || self.service.game.is_some() {
            self.service.error =
                Some("请先完成或取消当前任务，并退出正在运行的游戏，再拖入整合包。".into());
            return;
        }
        if self.page == Page::Downloads {
            self.category = 1;
        }
        self.service
            .content_drop_install(&files[0], &self.settings, ctx);
    }

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

    fn top_bar(&mut self, ui: &mut Ui, ctx: &Context) {
        egui::Panel::top("topbar")
            .exact_size(76.0)
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
                let brand_width = if use_short_name { 160.0 } else { 340.0 };
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
                            ui.allocate_exact_size(Vec2::new(36.0, 36.0), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(star_rect, 11.0, theme::nav_active_bg());
                        theme::star4(ui.painter(), star_rect.center(), 12.0, theme::ACCENT);
                        if use_short_name {
                            ui.label(
                                RichText::new("NSCL")
                                    .size(16.0)
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

                let navigation_rect = egui::Rect::from_center_size(center, Vec2::new(476.0, 50.0));
                ui.painter()
                    .rect_filled(navigation_rect, 15.0, theme::INPUT_BG);
                ui.painter().rect_stroke(
                    navigation_rect,
                    15.0,
                    egui::Stroke::new(1.0, theme::CARD_BORDER),
                    egui::StrokeKind::Inside,
                );
                let mut nav = ui.new_child(
                    UiBuilder::new()
                        .max_rect(egui::Rect::from_center_size(
                            center,
                            Vec2::new(460.0, rect.height()),
                        ))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                nav.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(5.0, 0.0);
                    for item in Page::ALL {
                        let active = self.page == item
                            || (self.page == Page::Instances && item == Page::Home);
                        let response = navigation_button(ui, item, active);
                        if response.clicked() {
                            self.page = item;
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
                        ui.spacing_mut().item_spacing.x = 8.0;
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

fn navigation_button(ui: &mut Ui, page: Page, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(88.0, 38.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, page.title()));
    let fill = if selected {
        theme::nav_active_bg()
    } else if response.hovered() {
        theme::SECONDARY_BG
    } else {
        Color32::TRANSPARENT
    };
    let color = if selected {
        theme::NAV_ACTIVE
    } else {
        theme::NAV_TEXT
    };
    ui.painter().rect_filled(rect, 10.0, fill);
    theme::icon(
        ui.painter(),
        egui::Rect::from_min_size(rect.min + Vec2::new(14.0, 11.0), Vec2::splat(16.0)),
        page.icon(),
        color,
    );
    ui.painter().text(
        rect.min + Vec2::new(40.0, 19.0),
        egui::Align2::LEFT_CENTER,
        page.title(),
        egui::FontId::proportional(14.0),
        color,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            10.0,
            egui::Stroke::new(1.5, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn window_button(ui: &mut Ui, label: &str, hover: Color32, action: impl FnOnce()) {
    let response = styled_button(
        ui,
        RichText::new(label).size(14.0).color(theme::BODY_TEXT),
        &ButtonStyle {
            hover,
            active: hover,
            radius: 5.0,
            min_size: Vec2::new(36.0, 32.0),
            padding: Vec2::new(8.0, 6.0),
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
    pub padding: Vec2,
    pub stroke: Option<egui::Stroke>,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            fill: Color32::TRANSPARENT,
            hover: theme::HOVER,
            active: theme::HOVER,
            radius: 10.0,
            min_size: Vec2::ZERO,
            padding: Vec2::new(16.0, 10.0),
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
    let size = (galley.size() + style.padding * 2.0).max(style.min_size);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label));
    let hover = ui
        .ctx()
        .animate_bool(response.id.with("button-hover"), response.hovered());
    let fill = if response.is_pointer_button_down_on() {
        style.active
    } else {
        let from = style.fill.to_array();
        let to = style.hover.to_array();
        let color: [u8; 4] =
            std::array::from_fn(|i| egui::lerp(from[i] as f32..=to[i] as f32, hover) as u8);
        Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3])
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
    painter.rect_filled(
        rect,
        8.0,
        if muted {
            theme::SECONDARY_BG
        } else {
            theme::nav_active_bg()
        },
    );
    painter.rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(
            1.0,
            if muted {
                Color32::TRANSPARENT
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

pub fn tab_button(ui: &mut Ui, label: &str, selected: bool) -> bool {
    styled_button(
        ui,
        RichText::new(label).size(14.0).color(if selected {
            theme::ACCENT
        } else {
            theme::MUTED
        }),
        &ButtonStyle {
            fill: if selected {
                theme::nav_active_bg()
            } else {
                Color32::TRANSPARENT
            },
            hover: theme::HOVER,
            active: theme::HOVER,
            min_size: Vec2::new(104.0, theme::CONTROL_HEIGHT),
            stroke: selected.then_some(egui::Stroke::new(1.0, theme::BADGE_BORDER)),
            ..Default::default()
        },
    )
    .clicked()
}

pub fn heading(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(20.0)
            .family(theme::bold_family())
            .color(theme::HEADING),
    );
}

pub fn body(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(14.0).color(theme::MUTED));
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(theme::HINT));
}

fn initial_page() -> Page {
    #[cfg(debug_assertions)]
    if let Ok(title) = std::env::var("NSCL_PREVIEW_PAGE") {
        return Page::ALL
            .into_iter()
            .chain([Page::Instances])
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
