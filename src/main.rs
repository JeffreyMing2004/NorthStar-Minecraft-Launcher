#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod pages;
mod store;
mod theme;

use app::NsclApp;

fn main() -> eframe::Result {
    let viewport = egui::ViewportBuilder::default()
        .with_title("NorthStar Minecraft Launcher")
        .with_inner_size([1200.0, 760.0])
        .with_min_inner_size([960.0, 640.0])
        .with_decorations(false);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "NorthStar Minecraft Launcher",
        options,
        Box::new(|cc| {
            install_cjk_fonts(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(NsclApp::new()))
        }),
    )
}

// egui 内置字体不含 CJK 字形，运行时从系统加载中文字体
fn install_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [r"C:\Windows\Fonts\msyh.ttc", r"C:\Windows\Fonts\simhei.ttf"] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "cjk".into(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push("cjk".into());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}
