use super::*;

#[test]
fn runtime_install_progress_stays_cancellable_in_settings() {
    let (_root, mut app, ctx) = super::tests::fixture();
    app.service.task = Task::Install;
    app.service.progress = Progress {
        stage: "下载 Java 21".into(),
        bytes_downloaded: 1024 * 1024,
        total_bytes: Some(2 * 1024 * 1024),
        ..Default::default()
    };
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(876.0, 390.0),
            )),
            ..Default::default()
        },
        |ui| crate::pages::settings(ui, &mut app),
    );
    output.textures_delta.clear();
    let cancel = output
        .shapes
        .iter()
        .find_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape {
                if text.galley.job.text == "取消任务" {
                    Some(text.pos)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .expect("missing cancel button");
    assert!(cancel.y < 390.0);
    assert!(app.service.task == Task::Install);
    output.drop_without_applying_deltas();
}
