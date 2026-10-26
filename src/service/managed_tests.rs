use super::tests::{fixture, text_rect};
use super::*;
use crate::app::{NsclApp, Page};
use content::ContentMode;
use egui::{Context, FullOutput, Pos2, RawInput, Rect, Vec2};

fn instance_frame(ctx: &Context, app: &mut NsclApp, size: Vec2) -> FullOutput {
    ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        },
        |ui| {
            // Mirrors the central panel the app gives this page.
            ui.spacing_mut().item_spacing.y = 0.0;
            crate::pages::instance_windows(ui.ctx(), app);
            crate::instance_ui::show(ui, app);
        },
    )
}

#[test]
fn mod_tab_shows_update_preview_and_removal_controls_during_background_work() {
    let (_root, mut app, ctx) = fixture();
    app.page = Page::Instances;
    app.service.installed = vec![InstalledVersion {
        external: false,
        id: "test-version".into(),
        kind: "release".into(),
        java_major: 17,
        platform: String::new(),
        metadata_sha1: String::new(),
        loader: Some(LoaderVersion {
            minecraft: "1.20.1".into(),
            kind: LoaderKind::Fabric,
            version: "0.16.0".into(),
            stable: true,
        }),
        extra_files: vec![],
    }];
    app.service.instances = vec![crate::instances::Instance {
        id: "test".into(),
        name: "Test instance".into(),
        version_id: "test-version".into(),
        ..Default::default()
    }];
    app.service.selected = "test".into();
    app.instance_browser.selected = "test".into();
    app.instance_browser.tab = crate::instance_ui::DetailTab::Mods;
    app.service.content.mode = ContentMode::Instance;
    app.service.content.target = "test".into();
    let target = crate::content::Target {
        instance: crate::instances::Instance {
            id: "test".into(),
            name: "Test instance".into(),
            version_id: "test-version".into(),
            ..Default::default()
        },
        minecraft: "1.20.1".into(),
        loader: Some(LoaderKind::Fabric),
        loader_version: Some("0.16.0".into()),
    };
    let version:crate::content::ContentVersion=serde_json::from_value(serde_json::json!({"id":"v1","project_id":"example","name":"Example mod","version_number":"1.0","version_type":"release","date_published":"2026-01-01","game_versions":["1.20.1"],"loaders":["fabric"],"files":[]})).unwrap();
    let installed = crate::content::ManagedMod {
        version: version.clone(),
        file: crate::content::InstalledFile {
            path: "mods/example.jar".into(),
            hashes: Default::default(),
            size: 1,
        },
    };
    app.service.content.managed = Some(crate::content::ManagedCatalog {
        target,
        entries: vec![crate::content::ManagedEntry {
            installed,
            intact: true,
        }],
        other_files: vec!["manual.jar".into()],
    });
    let mut next = version;
    next.id = "v2".into();
    next.version_number = "2.0".into();
    app.service
        .content
        .updates
        .available
        .push(crate::content::UpdateCandidate {
            project_id: "example".into(),
            current_version: "1.0".into(),
            available: next,
        });
    app.service
        .content
        .managed_selection
        .insert("example".into());
    app.service.content.updates_checked = true;
    let size = egui::vec2(1116.0, 720.0);
    instance_frame(&ctx, &mut app, size).drop_without_applying_deltas();
    for task in [Task::Idle, Task::Content] {
        app.service.task = task;
        let mut output = instance_frame(&ctx, &mut app, size);
        output.textures_delta.clear();
        for label in ["卸载所选", "检查更新", "查看更新方案"] {
            let rect = text_rect(&output, label);
            assert!(
                rect.max.x < size.x && rect.min.y >= 0.0,
                "{label}: {rect:?}"
            );
        }
        if task == Task::Content {
            let cancel = text_rect(&output, "取消任务");
            assert!(cancel.max.y < size.y, "cancel: {cancel:?} size: {size:?}");
        }
        assert!(app.service.content.plan.is_none());
        assert_eq!(app.service.content.managed_selection.len(), 1);
        output.drop_without_applying_deltas();
    }
}
