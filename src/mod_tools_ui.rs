use crate::{app::NsclApp, service::mod_tools::Action};
pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    let Some(plan) = app.service.identify_plan.clone() else {
        return;
    };
    let mut open = true;
    let mut adopt = false;
    egui::Window::new("确认本地模组识别结果")
        .open(&mut open)
        .default_width(650.0)
        .vscroll(true)
        .max_height(520.0)
        .show(ctx, |ui| {
            ui.label(plan.target.label());
            ui.label("只上传文件哈希进行识别；确认后登记来源，不移动或替换本地 JAR。");
            for item in &plan.candidates {
                ui.label(format!(
                    "{} {} · {}",
                    item.version.name,
                    item.version.version_number,
                    if item.bundled {
                        "整合包自带"
                    } else {
                        "玩家追加"
                    }
                ));
                ui.label(&item.file.path);
            }
            for warning in &plan.warnings {
                ui.label(warning);
            }
            if ui
                .add_enabled(
                    !app.service.busy()
                        && app.service.game.is_none()
                        && !plan.candidates.is_empty(),
                    egui::Button::new("确认登记所识别模组"),
                )
                .clicked()
            {
                adopt = true;
            }
        });
    if adopt {
        app.service
            .mod_tools(Action::Adopt(Box::new(plan)), &app.settings, ctx);
    }
    if !open {
        app.service.identify_plan = None;
    }
}
