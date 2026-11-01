use crate::app::NsclApp;
pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    if !app.service.content.show_manual {
        return;
    }
    let mut open = true;
    let mut supply = false;
    egui::Window::new("手动补齐官方文件").open(&mut open).default_width(620.0).vscroll(true).max_height((ctx.content_rect().height() - 64.0).max(200.0)).show(ctx,|ui|{
        ui.label("这些文件未提供第三方下载地址。请从官方页面下载指定版本，填入本地文件路径后校验并补齐。");
        for version in &app.service.content.manual_required {
            ui.strong(&version.name);
            if let Some(file) = version.files.first() {
                ui.label(format!("{} · {:.1} MiB", file.filename, file.size as f64 / 1048576.0));
            }
            if let Some(url) = crate::content::curseforge_website(version) {
                ui.hyperlink_to("打开 CurseForge 官方文件页面", url);
            }
        }
        ui.add(egui::TextEdit::singleline(&mut app.manual_path).hint_text("下载后填写完整 JAR / ZIP 路径").desired_width(ui.available_width()));
        if ui.add_enabled(!app.service.busy(),egui::Button::new("校验并补齐此文件")).clicked(){supply=true;}
        if app.service.content.manual_required.is_empty(){ui.label("文件已补齐，可以关闭窗口并继续安装。");}
        if !app.service.notice.is_empty(){ui.label(&app.service.notice);}
    });
    if supply {
        app.service.content_manual(
            std::path::PathBuf::from(app.manual_path.trim().trim_matches('"')),
            &app.settings,
            ctx,
        );
    }
    app.service.content.show_manual = open;
}
