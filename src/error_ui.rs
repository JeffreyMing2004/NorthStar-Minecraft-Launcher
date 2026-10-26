use crate::{
    app::{self, NsclApp, Page},
    minecraft::ProxyMode,
    theme,
};

pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    let Some(error) = app.service.error.clone() else {
        return;
    };
    let advice = nscl::network::advice(&error);
    let can_retry =
        app.service.manifest_error.as_deref() == Some(error.as_str()) && !app.service.busy();
    let (mut open, mut retry, mut direct, mut settings) = (true, false, false, false);
    egui::Window::new("操作未完成")
        .open(&mut open)
        .default_width(580.0)
        .show(ctx, |ui| {
            if let Some(info) = &advice {
                app::heading(ui, info.title);
                ui.add_space(10.0);
                app::body(ui, info.help);
                ui.add_space(14.0);
                theme::toolbar(ui, |ui| {
                    if can_retry {
                        retry = ui.button("重试").clicked();
                        if info.proxy_related && app.settings.source.proxy_mode == ProxyMode::System
                        {
                            direct = ui
                                .button("切换直连并重试")
                                .on_hover_text("本次运行使用直连；需要长期生效时请在设置中保存。")
                                .clicked();
                        }
                    }
                    settings = ui.button("打开网络设置").clicked();
                });
                ui.add_space(10.0);
                egui::CollapsingHeader::new("技术详情").show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| {
                            ui.label(&error);
                        });
                });
            } else {
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        ui.label(&error);
                    });
            }
            if let Some(path) = &app.service.last_log {
                ui.label(format!("启动日志：{}", path.display()));
            }
            ui.add_space(10.0);
            if ui.button("复制错误信息").clicked() {
                ctx.copy_text(error.clone());
            }
        });
    if settings {
        app.page = Page::Settings;
    }
    if !open || settings {
        app.service.error = None;
    }
    if retry || direct {
        if direct {
            app.settings.source.proxy_mode = ProxyMode::Direct;
        }
        app.service.load_versions(&app.settings, ctx);
    }
}
