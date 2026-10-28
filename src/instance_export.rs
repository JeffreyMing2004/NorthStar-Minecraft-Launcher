use crate::{
    app::{self, NsclApp},
    content::PackExportOptions,
    instances::Instance,
    pages::download_button,
};
use std::path::PathBuf;

pub struct ExportDialog {
    instance: Instance,
    options: PackExportOptions,
    destination: String,
    error: Option<String>,
}

impl ExportDialog {
    pub fn new(instance: &Instance) -> Self {
        Self {
            instance: instance.clone(),
            options: PackExportOptions::for_instance(instance),
            destination: String::new(),
            error: None,
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    let Some(mut dialog) = app.instance_browser.export.take() else {
        return;
    };
    let editable = !app.service.busy() && app.service.game.is_none();
    let (mut open, mut started) = (true, false);
    egui::Window::new("导出整合包")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            app::hint(ui, &format!("来源实例：{}", dialog.instance.name));
            ui.add_enabled_ui(editable, |ui| {
                ui.label("整合包名称");
                ui.add(
                    egui::TextEdit::singleline(&mut dialog.options.name)
                        .desired_width(ui.available_width()),
                );
                ui.label("版本");
                ui.add(
                    egui::TextEdit::singleline(&mut dialog.options.version)
                        .desired_width(ui.available_width()),
                );
                ui.label("简介（可选）");
                ui.add(
                    egui::TextEdit::singleline(&mut dialog.options.summary)
                        .desired_width(ui.available_width()),
                );
                app::body(
                    ui,
                    "包含模组、配置、脚本、资源包与光影，保留模组的启用状态。",
                );
                ui.checkbox(&mut dialog.options.include_saves, "包含存档");
                ui.checkbox(
                    &mut dialog.options.include_settings,
                    "包含游戏设置与服务器列表",
                );
                app::hint(
                    ui,
                    "导出为客户端 .mrpack；导入时自动准备 Minecraft 与加载器。",
                );
                ui.label("保存为新文件（.mrpack）");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut dialog.destination)
                            .hint_text("输入完整保存路径")
                            .desired_width((ui.available_width() - 84.0).max(100.0)),
                    );
                    if download_button(ui, "浏览…", cfg!(windows)) {
                        let filename = format!("{}.mrpack", dialog.instance.id);
                        match crate::file_picker::save_pack(&filename) {
                            Ok(Some(path)) => {
                                dialog.destination = path.to_string_lossy().into_owned()
                            }
                            Ok(None) => {}
                            Err(error) => dialog.error = Some(error),
                        }
                    }
                });
                if let Some(error) = &dialog.error {
                    app::hint(ui, error);
                }
                if app::primary_button(ui, "开始导出") {
                    let result = dialog.options.validate().and_then(|()| {
                        if dialog.destination.trim().is_empty() {
                            Err("请选择保存位置。".into())
                        } else {
                            Ok(())
                        }
                    });
                    match result {
                        Err(error) => dialog.error = Some(error),
                        Ok(()) => {
                            app.service.export_pack(
                                &dialog.instance,
                                dialog.options.clone(),
                                PathBuf::from(dialog.destination.trim()),
                                &app.settings,
                                ctx,
                            );
                            started = app.service.busy();
                        }
                    }
                }
            });
        });
    if open && !started {
        app.instance_browser.export = Some(dialog);
    }
}
