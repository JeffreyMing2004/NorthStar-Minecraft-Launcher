use egui::{Color32, Context, Frame, Stroke};

pub const WINDOW_BG: Color32 = Color32::from_rgb(0x0c, 0x13, 0x1e);
pub const TOPBAR_BG: Color32 = Color32::from_rgb(0x10, 0x19, 0x25);
pub const CARD_BG: Color32 = Color32::from_rgb(0x14, 0x1f, 0x2d);
pub const CARD_BORDER: Color32 = Color32::from_rgb(0x22, 0x2c, 0x38);
pub const BODY_TEXT: Color32 = Color32::from_rgb(0xe4, 0xec, 0xf4);
pub const HEADING: Color32 = Color32::from_rgb(0xed, 0xf3, 0xfa);
pub const MUTED: Color32 = Color32::from_rgb(0x91, 0xa2, 0xb6);
pub const HINT: Color32 = Color32::from_rgb(0x82, 0x93, 0xa7);
pub const EYEBROW: Color32 = Color32::from_rgb(0x76, 0xb8, 0xbd);
pub const ACCENT: Color32 = Color32::from_rgb(0x80, 0xdf, 0xcd);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xa0, 0xf0, 0xdf);
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x10, 0x2b, 0x2d);
pub const NAV_TEXT: Color32 = Color32::from_rgb(0x95, 0xa6, 0xba);
pub const NAV_ACTIVE: Color32 = Color32::from_rgb(0x8d, 0xe9, 0xdc);
pub fn nav_active_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(0x76, 0xdb, 0xcb, 0x12)
}
pub const HOVER: Color32 = Color32::from_rgb(0x26, 0x39, 0x4b);
pub const SECONDARY_BG: Color32 = Color32::from_rgb(0x1b, 0x29, 0x39);
pub const SECONDARY_BORDER: Color32 = Color32::from_rgb(0x3a, 0x4c, 0x5e);
pub const BADGE_TEXT: Color32 = Color32::from_rgb(0x8d, 0xd3, 0xc9);
pub const BADGE_BORDER: Color32 = Color32::from_rgb(0x4a, 0x8a, 0x82);
pub const FOOTER_TEXT: Color32 = Color32::from_rgb(0x74, 0x87, 0x9d);
pub const DASHED_BORDER: Color32 = Color32::from_rgb(0x51, 0x66, 0x74);
pub fn dashed_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(0x0d, 0x16, 0x21, 0x55)
}
pub const LANDSCAPE_BASE: Color32 = Color32::from_rgb(0x1c, 0x3d, 0x4d);
pub const MOUNTAIN_BACK: Color32 = Color32::from_rgb(0x24, 0x50, 0x64);
pub const MOUNTAIN_FRONT: Color32 = Color32::from_rgb(0x16, 0x2e, 0x40);
pub const STAR: Color32 = Color32::from_rgb(0xa3, 0xf5, 0xdb);

pub fn card(padding: i8) -> Frame {
    Frame::default()
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0, CARD_BORDER))
        .corner_radius(16.0)
        .inner_margin(egui::Margin::same(padding))
}

pub fn apply(ctx: &Context) {
    ctx.all_styles_mut(|style| {
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = WINDOW_BG;
        style.visuals.window_fill = CARD_BG;
        style.visuals.faint_bg_color = Color32::from_rgb(0x11, 0x1b, 0x28);
        style.visuals.hyperlink_color = ACCENT;
        style.visuals.selection.bg_fill = Color32::from_rgb(0x1d, 0x3a, 0x3f);
        style.visuals.widgets.active.bg_fill = HOVER;
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
    });
}
