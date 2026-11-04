use crate::{app::NsclApp, download_tasks::State};
pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    if !app.tasks_open {
        return;
    }
    let mut open = true;
    let mut add = None;
    let mut retry = None;
    egui::Window::new("下载任务中心")
        .open(&mut open)
        .default_width(700.0)
        .vscroll(true)
        .max_height((ctx.content_rect().height() - 64.0).max(250.0))
        .show(ctx, |ui| {
            if app.service.busy() {
                ui.label(&app.service.progress.stage);
                ui.label(app.service.rate.label(&app.service.progress));
                ui.add_enabled_ui(
                    app.service.task != crate::service::Task::DeleteInstance,
                    |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .button(
                                    if app
                                        .settings
                                        .source
                                        .paused
                                        .load(std::sync::atomic::Ordering::Relaxed)
                                    {
                                        "继续当前任务"
                                    } else {
                                        "暂停当前任务"
                                    },
                                )
                                .clicked()
                            {
                                app.settings
                                    .source
                                    .paused
                                    .fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
                            }
                            if ui.button("取消当前任务").clicked() {
                                app.service.cancel();
                            }
                        });
                    },
                );
            }
            if let Some(e) = &app.service.queue.error {
                ui.label(e);
            }
            ui.horizontal(|ui| {
                if ui.button("继续安装队列").clicked() {
                    app.service.queue.enabled = true;
                }
                if ui.button("暂停后续队列").clicked() {
                    app.service.queue.enabled = false;
                }
                ui.label("重启后请手动继续队列。");
            });
            egui::CollapsingHeader::new("加入游戏安装任务").show(ui, |ui| {
                if app.service.versions.is_empty()
                    && ui
                        .add_enabled(!app.service.busy(), egui::Button::new("加载游戏版本"))
                        .clicked()
                {
                    app.service.load_versions(&app.settings, ctx);
                }
                egui::ComboBox::from_id_salt("queue-game")
                    .selected_text(if app.queue_version.is_empty() {
                        "选择 Minecraft 版本"
                    } else {
                        &app.queue_version
                    })
                    .show_ui(ui, |ui| {
                        for v in &app.service.versions {
                            ui.selectable_value(&mut app.queue_version, v.id.clone(), &v.id);
                        }
                    });
                if ui.button("加入安装队列").clicked() {
                    add = app
                        .service
                        .versions
                        .iter()
                        .find(|v| v.id == app.queue_version)
                        .cloned();
                }
            });
            ui.separator();
            if app.service.queue.items.is_empty() {
                ui.label("暂无安装记录。");
            }
            for item in app.service.queue.items.iter().rev() {
                ui.push_id(&item.id, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(&item.entry.id);
                        ui.label(
                            item.loader
                                .as_ref()
                                .map(|l| l.label())
                                .unwrap_or_else(|| "原版".into()),
                        );
                        ui.label(item.state.label());
                        if matches!(item.state, State::Failed | State::Cancelled)
                            && ui.button("重试").clicked()
                        {
                            retry = Some(item.id.clone());
                        }
                    });
                    if !item.message.is_empty() {
                        ui.label(&item.message);
                    }
                });
            }
        });
    if let Some(entry) = add {
        app.service.queue.enqueue(entry, None, false);
    }
    if let Some(id) = retry {
        if let Some(item) = app.service.queue.items.iter_mut().find(|i| i.id == id) {
            item.state = State::Queued;
            item.message.clear();
        }
        app.service.queue.enabled = true;
        app.service.queue.save();
    }
    app.tasks_open = open;
}
