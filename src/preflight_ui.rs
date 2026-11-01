use crate::{
    app::{NsclApp, Page},
    service::LaunchPrompt,
};
pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    if let Some(id) = app.service.resume_launch.take() {
        app.service.selected = id;
        app.service.launch(&app.settings, &app.accounts, ctx);
    }
    let Some(prompt) = app.service.launch_prompt.clone() else {
        return;
    };
    let open = crate::accessibility::dialog(ctx, "启动前检查", 500.0, |ui| match &prompt {
        LaunchPrompt::Account => {
            ui.label("需要先选择游戏身份。可登录 Microsoft 账号或创建离线账号。");
            if ui.button("前往账号页").clicked() {
                app.page = Page::Account;
                app.service.launch_prompt = None;
            }
        }
        LaunchPrompt::Java {
            instance,
            major,
            detail,
        } => {
            ui.label(format!("实例：{} · 需要 Java {major}", instance.name));
            ui.label("账号检查已通过，游戏目录可写；启动后仍会校验游戏文件与可用内存。");
            ui.label(detail);
            if cfg!(windows)
                && ui
                    .add_enabled(
                        !app.service.busy(),
                        egui::Button::new("安装匹配 Java 并继续启动"),
                    )
                    .clicked()
            {
                let mut instance = (**instance).clone();
                instance.java_path = Some(String::new());
                match crate::instances::save(
                    &crate::store::game_directory(&app.settings),
                    &instance,
                ) {
                    Ok(()) => {
                        app.service.refresh(&app.settings);
                        app.service.pending_java_launch = Some(instance.id);
                        app.service.java_install_major = *major;
                        app.service.install_java(ctx);
                    }
                    Err(e) => app.service.error = Some(e),
                }
            }
            if ui.button("手动选择 Java").clicked() {
                app.page = Page::Settings;
                app.service.launch_prompt = None;
            }
            if app.service.busy() {
                ui.spinner();
                ui.label(&app.service.progress.stage);
                if ui.button("取消安装").clicked() {
                    app.service.cancel();
                }
            }
        }
    });
    if !open {
        app.service.launch_prompt = None;
        app.service.pending_java_launch = None;
    }
    if app.service.launch_prompt.is_none() {
        crate::accessibility::finish_dialog(ctx, "启动前检查");
    }
}
