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

// 运行时从系统字体加载：Segoe UI 优先渲染拉丁字符（与原 Web 字体栈一致），
// 微软雅黑作为 CJK 回退；粗体族同理（Segoe Bold + 雅黑粗体）
fn install_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let insert_fallback = |fonts: &mut egui::FontDefinitions, name: &str, path: &str| -> bool {
        match std::fs::read(path) {
            Ok(bytes) => {
                fonts.font_data.insert(
                    name.into(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                true
            }
            Err(_) => false,
        }
    };
    let segoe = insert_fallback(&mut fonts, "segoe", r"C:\Windows\Fonts\segoeui.ttf");
    let cjk = insert_fallback(&mut fonts, "cjk", r"C:\Windows\Fonts\msyh.ttc");
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.entry(family).or_default();
        if segoe {
            list.insert(0, "segoe".into());
        }
        if cjk {
            list.push("cjk".into());
        }
    }
    let segoe_bold = insert_fallback(&mut fonts, "segoe-bold", r"C:\Windows\Fonts\segoeuib.ttf");
    let cjk_bold = insert_fallback(&mut fonts, "cjk-bold", r"C:\Windows\Fonts\msyhbd.ttc");
    let mut bold_list = Vec::new();
    if segoe_bold {
        bold_list.push("segoe-bold".into());
    }
    if cjk_bold {
        bold_list.push("cjk-bold".into());
    }
    bold_list.extend(fonts.families[&egui::FontFamily::Proportional].clone());
    if !bold_list.is_empty() {
        fonts.families.insert(
            egui::FontFamily::Name(std::sync::Arc::from("cjk-bold")),
            bold_list,
        );
    }
    ctx.set_fonts(fonts);
}
