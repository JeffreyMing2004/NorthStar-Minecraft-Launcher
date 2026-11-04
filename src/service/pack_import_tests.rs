use super::*;
use crate::{
    app::NsclApp,
    service::{
        Event,
        content::ContentEvent,
        tests::{fixture, text_rect},
    },
};

fn frame(
    ctx: &egui::Context,
    app: &mut NsclApp,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| {
            crate::pages::task_status(ui, app);
            crate::error_ui::show(ctx, app);
            crate::pack_import_ui::show(ctx, app);
        },
    );
    output.textures_delta.clear();
    output
}

fn click(ctx: &egui::Context, app: &mut NsclApp, size: egui::Vec2, pos: egui::Pos2) {
    frame(
        ctx,
        app,
        size,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ],
    )
    .drop_without_applying_deltas();
}

#[test]
fn pack_import_modal_replaces_inline_progress_and_supports_pause_cancel_at_small_sizes() {
    let (_root, mut app, ctx) = fixture();
    app.service
        .begin_pack_import("永生0公测V1.2客户端-下载包(需要拖入启动器下载).mrpack".into());
    app.service.progress.stage = "下载并校验整合包资源".into();
    app.service.progress.total = 7;
    app.service.progress.completed = 3;
    for size in [egui::vec2(1200.0, 760.0), egui::vec2(520.0, 420.0)] {
        frame(&ctx, &mut app, size, vec![]).drop_without_applying_deltas();
        let output = frame(&ctx, &mut app, size, vec![]);
        for label in [
            "正在导入整合包",
            "下载并校验整合包资源",
            "当前阶段：3 / 7 个文件",
            "暂停导入",
            "取消导入",
        ] {
            assert!(
                egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                    .contains_rect(text_rect(&output, label)),
                "{label}"
            );
        }
        assert_eq!(output.shapes.iter().filter(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == "下载并校验整合包资源")).count(), 1);
    }
    let size = egui::vec2(520.0, 420.0);
    let output = frame(&ctx, &mut app, size, vec![]);
    click(
        &ctx,
        &mut app,
        size,
        text_rect(&output, "暂停导入").center(),
    );
    assert!(
        app.settings
            .source
            .paused
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    let output = frame(&ctx, &mut app, size, vec![]);
    text_rect(&output, "继续导入");
    click(
        &ctx,
        &mut app,
        size,
        text_rect(&output, "取消导入").center(),
    );
    assert!(
        app.service
            .cancel
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    assert!(app.service.pack_import.as_ref().unwrap().cancelling);
    app.service.dismiss_pack_import();
    assert!(
        app.service.pack_import.is_some(),
        "a running import must stay visible"
    );
    app.service
        .sender
        .send(Event::Failed("操作已取消。".into()))
        .unwrap();
    app.service.poll(&app.settings);
    assert!(matches!(
        app.service.pack_import.as_ref().unwrap().status,
        PackImportStatus::Cancelled
    ));
    app.service.dismiss_pack_import();
    assert!(!app.service.busy());
}

#[test]
fn import_completion_and_failure_stay_in_the_dialog_until_acknowledged() {
    let (_root, mut app, ctx) = fixture();
    app.service.begin_pack_import("Test pack".into());
    app.service
        .sender
        .send(Event::Content(ContentEvent::Installed(
            crate::instances::Instance::default(),
            "整合包 Test pack 安装完成。".into(),
        )))
        .unwrap();
    app.service.poll(&app.settings);
    assert!(
        matches!(&app.service.pack_import.as_ref().unwrap().status, PackImportStatus::Succeeded(message) if message.contains("Test pack"))
    );
    assert!(!app.service.busy());
    app.service
        .queue
        .enqueue(app.service.versions[0].clone(), None, false);
    app.service.queue.enabled = true;
    app.service.run_queue(&app.settings, &ctx);
    assert!(
        app.service.queue.active.is_none(),
        "queued tasks must wait for the result dialog"
    );
    app.service.dismiss_pack_import();
    assert!(!app.service.busy());

    app.service.begin_pack_import("Bad pack".into());
    app.service
        .sender
        .send(Event::Failed("整合包缓存损坏。".into()))
        .unwrap();
    app.service.poll(&app.settings);
    assert!(
        matches!(&app.service.pack_import.as_ref().unwrap().status, PackImportStatus::Failed(error) if error == "整合包缓存损坏。")
    );
    let size = egui::vec2(960.0, 640.0);
    frame(&ctx, &mut app, size, vec![]).drop_without_applying_deltas();
    let output = frame(&ctx, &mut app, size, vec![]);
    text_rect(&output, "整合包导入失败");
    text_rect(&output, "整合包缓存损坏。");
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == "操作未完成"))
    );
    click(&ctx, &mut app, size, text_rect(&output, "关闭").center());
    assert!(app.service.pack_import.is_none());
    assert!(app.service.error.is_none());
}
