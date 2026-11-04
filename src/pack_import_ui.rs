use crate::{app::NsclApp, service::PackImportStatus, theme};
use std::{sync::atomic::Ordering, time::Duration};

pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    let Some(import) = &app.service.pack_import else {
        return;
    };
    let running = matches!(import.status, PackImportStatus::Running);
    let (mut cancel, mut pause, mut close) = (false, false, false);
    let paused = app.settings.source.paused.load(Ordering::Relaxed);
    let id = egui::Id::unique("pack-import-progress");
    let area = egui::Modal::default_area(id)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .constrain_to(ctx.content_rect());
    let response = egui::Modal::new(id).area(area).show(ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 64.0).clamp(160.0, 560.0));
        ui.horizontal(|ui| {
            if running {
                ui.spinner();
            }
            ui.heading(match &import.status {
                PackImportStatus::Running if import.cancelling => "正在取消导入",
                PackImportStatus::Running if paused => "整合包导入已暂停",
                PackImportStatus::Running => "正在导入整合包",
                PackImportStatus::Succeeded(_) => "整合包导入完成",
                PackImportStatus::Failed(_) => "整合包导入失败",
                PackImportStatus::Cancelled => "整合包导入已取消",
            });
        });
        ui.add_space(8.0);
        ui.label(&import.name);
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height((ctx.content_rect().height() - 260.0).max(100.0))
            .show(ui, |ui| match &import.status {
                PackImportStatus::Running => {
                    let progress = &app.service.progress;
                    if import.cancelling {
                        ui.label("正在停止任务并清理临时文件，请稍候……");
                    } else {
                        ui.label(&progress.stage);
                    }
                    ui.add_space(12.0);
                    let fraction = progress
                        .total_bytes
                        .map(|total| progress.bytes_downloaded as f32 / total.max(1) as f32)
                        .or_else(|| {
                            (progress.total > 0)
                                .then(|| progress.completed as f32 / progress.total as f32)
                        });
                    let mut bar = egui::ProgressBar::new(fraction.unwrap_or(0.0).clamp(0.0, 1.0))
                        .desired_width(ui.available_width())
                        .desired_height(12.0)
                        .animate(fraction.is_none() && !paused);
                    if fraction.is_some() {
                        bar = bar.show_percentage();
                    }
                    ui.add(bar);
                    if progress.total > 0 {
                        ui.label(format!(
                            "当前阶段：{} / {} 个文件",
                            progress.completed, progress.total
                        ));
                    }
                    if progress.bytes_downloaded > 0 {
                        ui.label(app.service.rate.label(progress));
                    }
                }
                PackImportStatus::Succeeded(message) => {
                    ui.label(message);
                }
                PackImportStatus::Failed(error) => {
                    if let Some(advice) = nscl::network::advice(error) {
                        ui.strong(advice.title);
                        ui.label(advice.help);
                        ui.add_space(8.0);
                    }
                    ui.label(error);
                    if ui.button("复制错误信息").clicked() {
                        ctx.copy_text(error.clone());
                    }
                }
                PackImportStatus::Cancelled => {
                    ui.label("导入已停止，可以重新选择整合包。已有实例保持不变。");
                }
            });
        ui.add_space(12.0);
        let elapsed = import.elapsed().as_secs();
        ui.label(
            egui::RichText::new(format!("已用时 {:02}:{:02}", elapsed / 60, elapsed % 60))
                .color(theme::muted()),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if running {
                pause = ui
                    .add_enabled(
                        !import.cancelling,
                        egui::Button::new(if paused {
                            "继续导入"
                        } else {
                            "暂停导入"
                        }),
                    )
                    .clicked();
                cancel = ui
                    .add_enabled(!import.cancelling, egui::Button::new("取消导入"))
                    .clicked();
            } else {
                let button = ui.button("关闭");
                if ctx.memory(|m| m.focused().is_none()) {
                    button.request_focus();
                }
                close = button.clicked();
            }
        });
    });
    // Keep the progress dialog visible until the worker finishes. Escape and
    // backdrop clicks must not hide an ongoing import or silently cancel it.
    if !running && response.is_top_modal {
        close |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    }
    if pause {
        app.settings
            .source
            .paused
            .fetch_xor(true, Ordering::Relaxed);
    }
    if cancel {
        app.service.cancel_pack_import();
    }
    if close {
        app.service.dismiss_pack_import();
    }
    if running {
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}
