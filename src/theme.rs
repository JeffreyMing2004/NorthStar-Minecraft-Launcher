use egui::{Color32, Context, Frame, Stroke};

pub const WINDOW_BG: Color32 = Color32::from_rgb(0x10, 0x16, 0x18);
pub const TOPBAR_BG: Color32 = Color32::from_rgb(0x14, 0x1b, 0x1d);
pub const CARD_BG: Color32 = Color32::from_rgb(0x1a, 0x22, 0x25);
pub const CARD_BORDER: Color32 = Color32::from_rgb(0x2c, 0x38, 0x39);
pub const BODY_TEXT: Color32 = Color32::from_rgb(0xe4, 0xed, 0xe8);
pub const HEADING: Color32 = Color32::from_rgb(0xf0, 0xf5, 0xf1);
pub const MUTED: Color32 = Color32::from_rgb(0xa0, 0xb1, 0xab);
pub const HINT: Color32 = Color32::from_rgb(0x87, 0x9b, 0x93);
pub const ACCENT: Color32 = Color32::from_rgb(0xb1, 0xed, 0xc7);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xc7, 0xf7, 0xd8);
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x16, 0x34, 0x25);
pub const NAV_TEXT: Color32 = Color32::from_rgb(0x9b, 0xac, 0xa5);
pub const NAV_ACTIVE: Color32 = ACCENT;
pub fn nav_active_bg() -> Color32 {
    Color32::from_rgb(0x2c, 0x40, 0x36)
}
pub const HOVER: Color32 = Color32::from_rgb(0x30, 0x40, 0x3b);
pub const SECONDARY_BG: Color32 = Color32::from_rgb(0x24, 0x30, 0x2e);
pub const SECONDARY_BORDER: Color32 = Color32::from_rgb(0x3a, 0x4a, 0x44);
pub const INPUT_BG: Color32 = Color32::from_rgb(0x13, 0x1b, 0x1c);
pub const BADGE_TEXT: Color32 = Color32::from_rgb(0xb0, 0xd9, 0xc0);
pub const BADGE_BORDER: Color32 = Color32::from_rgb(0x3d, 0x5a, 0x4b);
pub const FOOTER_TEXT: Color32 = Color32::from_rgb(0x7d, 0x92, 0x87);
pub const MOUNTAIN_BACK: Color32 = Color32::from_rgb(0x39, 0x60, 0x50);
pub const MOUNTAIN_FRONT: Color32 = Color32::from_rgb(0x1f, 0x40, 0x36);
pub const STAR: Color32 = Color32::from_rgb(0xe1, 0xef, 0xc6);
pub const CONTROL_HEIGHT: f32 = 40.0;
pub const CONTROL_GAP: f32 = 12.0;
pub const TAB_GAP: f32 = 8.0;

pub fn toolbar<R>(
    ui: &mut egui::Ui,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(CONTROL_GAP, CONTROL_GAP);
        content(ui)
    })
}

pub fn card(padding: i8) -> Frame {
    Frame::default()
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0, CARD_BORDER))
        .corner_radius(18.0)
        .inner_margin(egui::Margin::same(padding))
}

// 微软雅黑粗体注册为独立字体族（见 main.rs），标题文字经此使用真实粗体字重
pub fn bold_family() -> egui::FontFamily {
    egui::FontFamily::Name(std::sync::Arc::from("cjk-bold"))
}

// 四角星：参照图中的 ✦ 为细长四角星，字体回退字形不一致，改为手绘
pub fn star4(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    let ratio = 0.25;
    let points = [
        (0.0f32, -1.0f32),
        (ratio, -ratio),
        (1.0, 0.0),
        (ratio, ratio),
        (0.0, 1.0),
        (-ratio, ratio),
        (-1.0, 0.0),
        (-ratio, -ratio),
    ];
    let screen = |(x, y): (f32, f32)| egui::pos2(center.x + x * radius, center.y + y * radius);
    let vertices: Vec<egui::Pos2> = points.iter().map(|&p| screen(p)).collect();
    for i in 0..vertices.len() {
        let next = vertices[(i + 1) % vertices.len()];
        painter.add(egui::Shape::convex_polygon(
            vec![center, vertices[i], next],
            color,
            egui::Stroke::NONE,
        ));
    }
}

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    User,
    Download,
    Settings,
    More,
    Folder,
    Java,
    Memory,
}

pub fn icon(p: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
    let point = |x, y| rect.min + egui::vec2(x * rect.width(), y * rect.height());
    let stroke = Stroke::new(1.6, color);
    let line = |points: &[(f32, f32)]| {
        p.add(egui::Shape::line(
            points.iter().map(|&(x, y)| point(x, y)).collect(),
            stroke,
        ));
    };
    match icon {
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![point(0.28, 0.18), point(0.82, 0.5), point(0.28, 0.82)],
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        Icon::User => {
            p.circle_stroke(point(0.5, 0.3), rect.width() * 0.17, stroke);
            line(&[
                (0.16, 0.87),
                (0.16, 0.78),
                (0.28, 0.6),
                (0.72, 0.6),
                (0.84, 0.78),
                (0.84, 0.87),
            ]);
        }
        Icon::Download => {
            line(&[(0.5, 0.12), (0.5, 0.65)]);
            line(&[(0.28, 0.45), (0.5, 0.67), (0.72, 0.45)]);
            line(&[(0.13, 0.7), (0.13, 0.87), (0.87, 0.87), (0.87, 0.7)]);
        }
        Icon::Settings => {
            p.circle_stroke(rect.center(), rect.width() * 0.3, stroke);
            p.circle_stroke(rect.center(), rect.width() * 0.1, stroke);
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::TAU / 8.0;
                let direction = egui::vec2(angle.cos(), angle.sin());
                p.line_segment(
                    [
                        rect.center() + direction * rect.width() * 0.3,
                        rect.center() + direction * rect.width() * 0.43,
                    ],
                    stroke,
                );
            }
        }
        Icon::More => {
            for (x, y) in [(0.29, 0.29), (0.71, 0.29), (0.29, 0.71), (0.71, 0.71)] {
                p.rect_stroke(
                    egui::Rect::from_center_size(point(x, y), rect.size() * 0.23),
                    2.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        Icon::Folder => {
            line(&[
                (0.1, 0.8),
                (0.1, 0.22),
                (0.42, 0.22),
                (0.52, 0.35),
                (0.9, 0.35),
                (0.9, 0.8),
                (0.1, 0.8),
            ]);
        }
        Icon::Java => {
            line(&[(0.2, 0.4), (0.2, 0.76), (0.7, 0.76), (0.7, 0.4), (0.2, 0.4)]);
            line(&[(0.7, 0.44), (0.9, 0.44), (0.9, 0.65), (0.7, 0.65)]);
            line(&[(0.34, 0.27), (0.4, 0.16)]);
            line(&[(0.53, 0.27), (0.59, 0.16)]);
            line(&[(0.1, 0.9), (0.8, 0.9)]);
        }
        Icon::Memory => {
            p.rect_stroke(
                rect.shrink(rect.width() * 0.23),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            for i in 0..3 {
                let t = 0.33 + i as f32 * 0.17;
                for (a, b) in [
                    ((0.08, t), (0.23, t)),
                    ((0.77, t), (0.92, t)),
                    ((t, 0.08), (t, 0.23)),
                    ((t, 0.77), (t, 0.92)),
                ] {
                    line(&[a, b]);
                }
            }
        }
    }
}

pub fn section_heading(ui: &mut egui::Ui, title: &str, symbol: Icon) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 10.0, nav_active_bg());
        icon(ui.painter(), rect.shrink(8.0), symbol, ACCENT);
        ui.label(
            egui::RichText::new(title)
                .size(17.0)
                .family(bold_family())
                .color(HEADING),
        );
    });
    ui.add_space(18.0);
}

pub fn apply(ctx: &Context) {
    ctx.all_styles_mut(|style| {
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = WINDOW_BG;
        style.visuals.window_fill = CARD_BG;
        style.visuals.faint_bg_color = TOPBAR_BG;
        style.visuals.hyperlink_color = ACCENT;
        style.visuals.selection.bg_fill = nav_active_bg();
        style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        style.visuals.window_corner_radius = egui::CornerRadius::same(16);
        style.visuals.window_stroke = Stroke::new(1.0, CARD_BORDER);
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, CARD_BORDER);
        for visuals in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            visuals.corner_radius = egui::CornerRadius::same(10);
            visuals.bg_stroke = Stroke::new(1.0, SECONDARY_BORDER);
            visuals.fg_stroke = Stroke::new(1.0, BODY_TEXT);
        }
        style.visuals.widgets.inactive.bg_fill = SECONDARY_BG;
        style.visuals.widgets.inactive.weak_bg_fill = SECONDARY_BG;
        style.visuals.widgets.hovered.bg_fill = HOVER;
        style.visuals.widgets.hovered.weak_bg_fill = HOVER;
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.65));
        style.visuals.widgets.active.bg_fill = HOVER;
        style.visuals.widgets.active.weak_bg_fill = HOVER;
        style.spacing.item_spacing = egui::vec2(CONTROL_GAP, CONTROL_GAP);
        style.spacing.button_padding = egui::vec2(16.0, 10.0);
        style.spacing.interact_size.y = CONTROL_HEIGHT;
        style.spacing.icon_width = 16.0;
        style.animation_time = 0.12;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style.spacing.slider_rail_height = 3.0;
        style.visuals.override_text_color = Some(BODY_TEXT);
        style.visuals.extreme_bg_color = INPUT_BG;
    });
}

pub fn spaced_text(
    painter: &egui::Painter,
    position: egui::Pos2,
    text: &str,
    size: f32,
    spacing: f32,
    color: Color32,
) {
    let job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    let galley = painter.layout_job(job);
    painter.galley(position, galley, color);
}
