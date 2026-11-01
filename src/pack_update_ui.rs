use crate::{
    app::{self, NsclApp},
    instances::Instance,
    service::pack_updates::Input,
};
pub struct Dialog {
    instance: Instance,
    requested: bool,
    path: String,
}
impl Dialog {
    pub fn new(instance: &Instance) -> Self {
        Self {
            instance: instance.clone(),
            requested: false,
            path: String::new(),
        }
    }
}
pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    let Some(mut dialog) = app.instance_browser.pack_update.take() else {
        return;
    };
    let mut open = true;
    if let Some(current) = app
        .service
        .instances
        .iter()
        .find(|i| i.id == dialog.instance.id)
        && dialog.instance.version_id != current.version_id
    {
        dialog.instance = current.clone();
        dialog.requested = false;
    }
    if (!dialog.requested || app.service.pack_state.instance != dialog.instance.id)
        && !app.service.busy()
    {
        app.service.pack_load(&dialog.instance, &app.settings, ctx);
        dialog.requested = true;
    }
    let editable = !app.service.busy() && app.service.game.is_none();
    let mut preview = None;
    let mut apply = false;
    egui::Window::new("整合包版本与升级")
        .open(&mut open)
        .default_width(700.0)
        .vscroll(true)
        .max_height((ctx.content_rect().height() - 64.0).max(250.0))
        .show(ctx, |ui| {
            app::heading(ui, &dialog.instance.name);
            ui.label(format!(
                "当前整合包版本：{}",
                app.service.pack_state.current
            ));
            ui.label(
                "升级前自动创建恢复点，保留存档与用户追加内容。历史回退在「备份与复制」中完成。",
            );
            if let Some(plan) = &mut app.service.pack_state.plan {
                ui.strong(format!(
                    "{} → {} · Minecraft {}",
                    plan.previous_version, plan.next.version, plan.next.minecraft
                ));
                ui.collapsing("更新日志", |ui| {
                    ui.label(&plan.next.changelog);
                });
                egui::CollapsingHeader::new(format!("{} 项包文件变更", plan.changes.len())).show(
                    ui,
                    |ui| {
                        for line in &plan.changes {
                            ui.label(line);
                        }
                    },
                );
                if !plan.conflicts.is_empty() {
                    ui.label("以下文件已被本地修改或属于存档/设置，默认保留：");
                    for path in &plan.conflicts {
                        ui.label(path);
                    }
                    ui.add_enabled(
                        editable,
                        egui::Checkbox::new(
                            &mut plan.replace_modified,
                            "使用新版包文件替换本地修改（存档、截图和游戏设置仍保留）",
                        ),
                    );
                }
                if !plan.next.manual_files.is_empty() && ui.button("补齐受限文件").clicked() {
                    app.service.content.manual_required = plan.next.manual_files.clone();
                    app.service.content.show_manual = true;
                }
                if ui
                    .add_enabled(editable, egui::Button::new("备份并确认升级"))
                    .clicked()
                {
                    apply = true;
                }
                if ui
                    .add_enabled(editable, egui::Button::new("返回版本选择"))
                    .clicked()
                {
                    app.service.pack_state.plan = None;
                }
            } else {
                if ui
                    .add_enabled(editable, egui::Button::new("刷新发布版本"))
                    .clicked()
                {
                    dialog.requested = false;
                }
                if app.service.pack_state.origin.is_none() {
                    ui.label("此本地包没有在线来源记录，可从本地新版包受控升级。");
                }
                let versions = app.service.pack_state.versions.clone();
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for v in versions {
                            ui.push_id(&v.id, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(format!(
                                        "{} · {} · {}",
                                        v.version_number,
                                        v.version_type,
                                        v.game_versions.join(",")
                                    ));
                                    if ui
                                        .add_enabled(editable, egui::Button::new("预览此版本"))
                                        .clicked()
                                    {
                                        preview = Some(Input::Online(Box::new(v.clone())));
                                    }
                                });
                            });
                        }
                    });
                ui.label("本地新版 .mrpack / CurseForge ZIP 路径");
                ui.text_edit_singleline(&mut dialog.path);
                if ui
                    .add_enabled(
                        editable && !dialog.path.trim().is_empty(),
                        egui::Button::new("预览本地升级"),
                    )
                    .clicked()
                {
                    preview = Some(Input::Local(dialog.path.trim().trim_matches('"').into()));
                }
            }
            if app.service.busy() {
                ui.spinner();
                ui.label(&app.service.progress.stage);
                if ui.button("取消任务").clicked() {
                    app.service.cancel();
                }
            }
            if !app.service.notice.is_empty() {
                ui.label(&app.service.notice);
            }
        });
    if let Some(input) = preview {
        app.service
            .pack_preview(&dialog.instance, input, &app.settings, ctx);
    }
    if apply {
        app.service.pack_apply(&app.settings, ctx);
    }
    if open {
        app.instance_browser.pack_update = Some(dialog);
    }
}
