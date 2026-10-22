use super::*;

#[test]
fn account_and_settings_ui_show_offline_accounts_and_automatic_controls() {
    let (_root, mut app, ctx) = super::tests::fixture();
    app.accounts.list = Default::default();
    app.accounts.list.add_offline("LocalPlayer").unwrap();
    app.settings.online_mode = false;
    app.settings.auto_java = true;
    app.settings.auto_memory = true;
    assert!(app.accounts.launch_account().is_none());
    for account_page in [true, false] {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(876.0, 2000.0),
                )),
                ..Default::default()
            },
            |ui| {
                if account_page {
                    crate::pages::account(ui, &mut app);
                } else {
                    crate::pages::settings(ui, &mut app);
                }
            },
        );
        output.textures_delta.clear();
        for label in if account_page {
            ["创建离线账号", "LocalPlayer", "当前账号"]
        } else {
            ["按游戏版本自动选择 Java", "自动分配游戏内存", "保存设置"]
        } {
            let rect = super::tests::text_rect(&output, label);
            assert!(rect.max.x < 876.0, "{label}: {rect:?}");
        }
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.job.text == "离线测试玩家名"
            || text.galley.job.text == "以 Minecraft 试玩模式启动"
            || text.galley.job.text == "使用离线测试身份")));
        assert!(!app.accounts.pending && !app.skin.pending);
        output.drop_without_applying_deltas();
    }
}

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
