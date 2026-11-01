use crate::{
    app::{self, NsclApp},
    instances::{
        Instance,
        backup::{self, CloneOptions, Scope},
    },
    service::backup::Action,
    store,
};

pub struct Dialog {
    instance: Instance,
    options: CloneOptions,
    confirm: Option<String>,
    loading: bool,
}
impl Dialog {
    pub fn new(instance: &Instance) -> Self {
        Self {
            instance: instance.clone(),
            options: CloneOptions {
                name: format!("{} 副本", instance.name).chars().take(64).collect(),
                saves: true,
                screenshots: false,
                servers: true,
            },
            confirm: None,
            loading: false,
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    let Some(mut dialog) = app.instance_browser.backup.take() else {
        return;
    };
    let editable = !app.service.busy() && app.service.game.is_none();
    let root = store::game_directory(&app.settings);
    let mut action = None;
    let mut close = false;
    if !app.service.busy() && !dialog.loading {
        action = Some(Action::List);
        dialog.loading = true;
    }
    if !app.service.busy()
        && let Some(current) = app
            .service
            .instances
            .iter()
            .find(|i| i.id == dialog.instance.id)
    {
        dialog.instance = current.clone();
    }
    let open = crate::accessibility::dialog(ctx, "备份、恢复与复制实例", 650.0, |ui| {
        app::body(ui, &format!("来源实例：{}", dialog.instance.name));
        if let Ok(p) = dialog.instance.game_directory(&root) {
            app::hint(ui, &format!("实际游戏目录：{}", p.display()));
        }
        if dialog.instance.isolation != crate::instances::Isolation::Isolated {
            ui.label("此实例复用共享/版本目录，备份和恢复会影响使用该目录的其他实例。");
        }
        app::hint(
            ui,
            "包含存档、模组、配置、脚本、资源包、光影、截图及游戏设置等已知内容目录；不包含游戏本体、账号、日志和缓存。自定义根目录内容请另行保存。仅在游戏退出后修改。",
        );
        ui.label("备份保存目录（留空使用启动器数据目录下的 backups）");
        ui.add_enabled(
            editable,
            egui::TextEdit::singleline(&mut app.settings.backup_directory)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(editable, egui::Button::new("保存位置并刷新"))
                .clicked()
            {
                match store::save(&app.settings) {
                    Ok(()) => {
                        action = Some(Action::List);
                        app.service.backup_snapshots.clear();
                        dialog.confirm = None;
                    }
                    Err(e) => app.service.error = Some(e),
                }
            }
            if ui
                .add_enabled(editable, egui::Button::new("备份实例"))
                .clicked()
            {
                action = Some(Action::Create(Scope::Instance));
            }
            if ui
                .add_enabled(editable, egui::Button::new("备份存档"))
                .clicked()
            {
                action = Some(Action::Create(Scope::Saves));
            }
            if ui.button("打开备份目录").clicked()
                && let Err(e) = open::that(backup::directory(&app.settings))
            {
                app.service.error = Some(e.to_string());
            }
        });
        ui.separator();
        ui.label(format!(
            "已保留 {} 份备份；恢复点不会自动删除。",
            app.service.backup_snapshots.len()
        ));
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                if app.service.backup_instance == dialog.instance.id {
                    for s in &app.service.backup_snapshots {
                        ui.horizontal_wrapped(|ui| {
                            let minutes = crate::accounts::now().saturating_sub(s.created) / 60;
                            ui.label(format!(
                                "{}{} · {minutes} 分钟前 · {:.1} MiB",
                                s.scope.label(),
                                if s.recovery { "（恢复前）" } else { "" },
                                s.bytes as f64 / 1048576.0
                            ));
                            if ui
                                .add_enabled(editable, egui::Button::new("恢复此备份"))
                                .clicked()
                            {
                                dialog.confirm = Some(s.id.clone());
                            }
                        });
                    }
                }
            });
        if let Some(id) = dialog.confirm.clone() {
            ui.label("确认替换此备份范围内的数据？会先保留当前数据的恢复点。共享目录中的同类内容也会替换。");
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(editable, egui::Button::new("确认恢复"))
                    .clicked()
                {
                    action = Some(Action::Restore(id));
                    dialog.confirm = None;
                }
                if ui.button("取消恢复").clicked() {
                    dialog.confirm = None;
                }
            });
        }
        ui.separator();
        ui.label("复制为新的独立实例");
        ui.add_enabled_ui(editable, |ui| {
            ui.text_edit_singleline(&mut dialog.options.name);
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut dialog.options.saves, "复制存档");
                ui.checkbox(&mut dialog.options.screenshots, "复制截图");
                ui.checkbox(&mut dialog.options.servers, "复制服务器列表");
            });
            if app::primary_button(ui, "开始复制") {
                action = Some(Action::Clone(dialog.options.clone()));
                close = true;
            }
        });
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
    if let Some(action) = action {
        if !matches!(action, Action::List) {
            dialog.loading = false;
        }
        app.service
            .backup_action(&dialog.instance, action, &app.settings, ctx);
    }
    if open && !close {
        app.instance_browser.backup = Some(dialog);
    } else {
        crate::accessibility::finish_dialog(ctx, "备份、恢复与复制实例");
    }
}
