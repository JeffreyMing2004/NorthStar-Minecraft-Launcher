use crate::{
    app::NsclApp,
    instances::{self, Instance, Isolation},
    pages,
};
use egui::{Context, Event, FullOutput, Pos2, RawInput, Rect, Vec2};

fn frame(ctx: &Context, app: &mut NsclApp, events: Vec<Event>) -> FullOutput {
    ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100., 850.))),
            events,
            ..Default::default()
        },
        |_| pages::instance_windows(ctx, app),
    )
}
fn click(ctx: &Context, app: &mut NsclApp, pos: Pos2) -> FullOutput {
    frame(
        ctx,
        app,
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ],
    )
}

#[test]
fn instance_configuration_saves_isolation_without_changing_existing_files() {
    let (root, mut app, ctx) = super::tests::fixture();
    let instance = Instance {
        id: "ui-test".into(),
        name: "UI test".into(),
        version_id: "1.20.1".into(),
        ..Default::default()
    };
    instances::save(root.path(), &instance).unwrap();
    let old = instance.directory(root.path()).unwrap().join("options.txt");
    std::fs::write(&old, b"previous options").unwrap();
    app.service.refresh(&app.settings);
    app.instance_edit = Some(instance.clone());
    frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
    let output = frame(&ctx, &mut app, vec![]);
    let combo = super::tests::text_rect(&output, Isolation::Isolated.label()).center();
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, combo).drop_without_applying_deltas();
    let output = frame(&ctx, &mut app, vec![]);
    let shared = super::tests::text_rect(&output, Isolation::Shared.label()).center();
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, shared).drop_without_applying_deltas();
    let output = frame(&ctx, &mut app, vec![]);
    let save = super::tests::text_rect(&output, "保存实例配置").center();
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, save).drop_without_applying_deltas();
    assert!(app.instance_edit.is_none());
    let saved = instances::list(root.path())
        .into_iter()
        .find(|i| i.id == instance.id)
        .unwrap();
    assert_eq!(saved.isolation, Isolation::Shared);
    assert_eq!(saved.game_directory(root.path()).unwrap(), root.path());
    assert_eq!(std::fs::read(old).unwrap(), b"previous options");
    assert!(app.service.error.is_none());
}

#[test]
fn installation_uses_new_defaults_but_repair_preserves_a_legacy_versions_mode() {
    for existing in [false, true] {
        let (root, mut app, _ctx) = super::tests::fixture();
        let version = crate::minecraft::InstalledVersion {
            external: false,
            id: "1.20.1".into(),
            kind: "release".into(),
            java_major: 17,
            platform: "test".into(),
            metadata_sha1: String::new(),
            loader: None,
            extra_files: vec![],
        };
        let folder = root.path().join("versions/1.20.1");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("1.20.1.json"), b"{}").unwrap();
        crate::store::write_json(&folder.join("nscl-instance.json"), &version).unwrap();
        if existing {
            app.service.refresh(&app.settings);
        }
        app.settings.default_isolation = Isolation::Shared;
        app.service
            .sender
            .send(super::Event::Installed(version))
            .unwrap();
        app.service.poll(&app.settings);
        let instance = app
            .service
            .instances
            .iter()
            .find(|i| i.id == app.service.selected)
            .unwrap();
        assert_eq!(
            instance.isolation,
            if existing {
                Isolation::Isolated
            } else {
                Isolation::Shared
            }
        );
        assert!(
            instance
                .directory(root.path())
                .unwrap()
                .join("nscl-instance.json")
                .is_file()
        );
    }
}
