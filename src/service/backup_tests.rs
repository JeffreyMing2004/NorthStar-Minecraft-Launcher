use crate::{
    instances::{
        Instance,
        backup::{self, CloneOptions, Scope},
    },
    service::backup::Action,
};
use std::time::{Duration, Instant};

fn finish(app: &mut crate::app::NsclApp) {
    let until = Instant::now() + Duration::from_secs(15);
    while app.service.busy() && Instant::now() < until {
        app.service.poll(&app.settings);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.service.busy());
    assert!(app.service.error.is_none(), "{:?}", app.service.error);
}

#[test]
fn backup_service_clones_the_browsed_instance_and_locks_mutation_during_game() {
    let (root, mut app, ctx) = super::tests::fixture();
    app.settings.backup_directory = root.path().join("backups").to_string_lossy().into_owned();
    let source = Instance {
        id: "source".into(),
        name: "Source".into(),
        version_id: "test-version".into(),
        ..Default::default()
    };
    crate::instances::save(root.path(), &source).unwrap();
    let file = source
        .directory(root.path())
        .unwrap()
        .join("saves/world/level.dat");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "world").unwrap();
    app.service.refresh(&app.settings);
    app.service
        .backup_action(&source, Action::Create(Scope::Saves), &app.settings, &ctx);
    finish(&mut app);
    assert_eq!(
        backup::list(&backup::directory(&app.settings), &source)
            .unwrap()
            .len(),
        1
    );
    app.service.backup_action(
        &source,
        Action::Clone(CloneOptions {
            name: "Independent".into(),
            saves: true,
            screenshots: false,
            servers: false,
        }),
        &app.settings,
        &ctx,
    );
    finish(&mut app);
    assert_ne!(app.service.selected, source.id);
    assert!(
        app.service
            .instances
            .iter()
            .any(|i| i.id == app.service.selected && i.name == "Independent")
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "world");
    app.service.game = Some((123, "unused.log".into()));
    app.service.backup_action(
        &source,
        Action::Create(Scope::Instance),
        &app.settings,
        &ctx,
    );
    assert!(!app.service.busy());
    assert_eq!(
        backup::list(&backup::directory(&app.settings), &source)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn backup_dialog_is_bounded_and_does_not_start_copy_on_open() {
    let (root, mut app, ctx) = super::tests::fixture();
    app.settings.backup_directory = root.path().join("backups").to_string_lossy().into_owned();
    let instance = Instance {
        id: "test".into(),
        name: "Test".into(),
        version_id: "test-version".into(),
        ..Default::default()
    };
    crate::instances::save(root.path(), &instance).unwrap();
    app.service.refresh(&app.settings);
    app.instance_browser.backup = Some(crate::instance_backup::Dialog::new(&instance));
    for pass in 0..3 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                ..Default::default()
            },
            |ui| crate::instance_backup::show(ui.ctx(), &mut app),
        );
        output.textures_delta.clear();
        // Floating windows need one sizing pass before their contents are painted.
        if pass == 0 {
            continue;
        }
        let rect = super::tests::text_rect(&output, "备份实例");
        assert!(
            rect.min.y >= 0.0 && rect.max.y < 640.0 && rect.max.x < 960.0,
            "{rect:?}"
        );
        output.drop_without_applying_deltas();
    }
    finish(&mut app);
    assert!(
        backup::list(&backup::directory(&app.settings), &instance)
            .unwrap()
            .is_empty()
    );
    assert_eq!(app.service.instances.len(), 1);
}
