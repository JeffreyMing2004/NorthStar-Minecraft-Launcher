use crate::{app::NsclApp, instances::Instance};
pub struct Dialog {
    instance: Instance,
    forge: String,
    path: String,
    confirmed: bool,
}
impl Dialog {
    pub fn new(root: &std::path::Path, instance: &Instance) -> Self {
        Self {
            instance: instance.clone(),
            forge: crate::content::Target::from_instance(root, instance)
                .ok()
                .and_then(|t| t.loader_version)
                .unwrap_or_default(),
            path: String::new(),
            confirmed: false,
        }
    }
}
pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    let Some(mut dialog) = app.instance_browser.optifine.take() else {
        return;
    };
    let mut install = false;
    let open = crate::accessibility::dialog(ctx, "本地 OptiFine 安装", 620.0, |ui| {
        ui.label(format!(
            "实例：{} · Forge {}",
            dialog.instance.name, dialog.forge
        ));
        ui.hyperlink_to("打开 OptiFine 官方下载页", "https://optifine.net/downloads");
        ui.label("手动下载匹配此 Minecraft 版本的官方 JAR，并核对下载页的 Forge 兼容说明。导入时校验官方文件名中的游戏版本和 JAR 标记；兼容性不能只靠文件名保证。");
        ui.add(
            egui::TextEdit::singleline(&mut dialog.path)
                .hint_text("OptiFine_游戏版本_HD_….jar 完整路径")
                .desired_width(ui.available_width()),
        );
        ui.checkbox(
            &mut dialog.confirmed,
            "我已核对该 OptiFine 构建支持当前 Forge 版本",
        );
        if ui
            .add_enabled(
                dialog.confirmed && !app.service.busy() && app.service.game.is_none(),
                egui::Button::new("确认导入到此实例"),
            )
            .clicked()
        {
            install = true;
        }
        if app.service.busy() {
            ui.label(&app.service.progress.stage);
            if ui.button("取消").clicked() {
                app.service.cancel();
            }
        }
        if let Some(e) = &app.service.error {
            ui.label(e);
        }
        if !app.service.notice.is_empty() {
            ui.label(&app.service.notice);
        }
    });
    if install {
        app.service.optifine_import(
            dialog.instance.clone(),
            dialog.path.trim().trim_matches('"').into(),
            dialog.forge.clone(),
            &app.settings,
            ctx,
        );
    }
    if open {
        app.instance_browser.optifine = Some(dialog);
    }
}
