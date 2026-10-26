use super::*;
use crate::{
    minecraft::{InstalledVersion, LoaderKind, LoaderVersion},
    service::{
        content::ContentMode,
        tests::{fixture, text_rect},
    },
};
use egui::{Context, Event, FullOutput, Pos2, RawInput, Rect};

fn populated() -> (tempfile::TempDir, NsclApp, Context) {
    let (root, mut app, ctx) = fixture();
    for (id, loader) in [
        ("1.20.1", None),
        (
            "1.20.1-fabric",
            Some(LoaderVersion {
                minecraft: "1.20.1".into(),
                kind: LoaderKind::Fabric,
                version: "0.16.0".into(),
                stable: true,
            }),
        ),
    ] {
        let directory = root.path().join("versions").join(id);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(format!("{id}.json")), b"{}").unwrap();
        crate::store::write_json(
            &directory.join("nscl-instance.json"),
            &InstalledVersion {
                external: false,
                id: id.into(),
                kind: "release".into(),
                java_major: 17,
                platform: String::new(),
                metadata_sha1: String::new(),
                loader,
                extra_files: vec![],
            },
        )
        .unwrap();
    }
    for (id, name, version) in [
        ("vanilla", "Vanilla world", "1.20.1"),
        ("modded", "Fabric world", "1.20.1-fabric"),
        ("pack-example", "Test pack", "1.20.1-fabric"),
    ] {
        instances::save(
            root.path(),
            &Instance {
                id: id.into(),
                name: name.into(),
                version_id: version.into(),
                ..Default::default()
            },
        )
        .unwrap();
    }
    app.service.refresh(&app.settings);
    app.service.selected = "vanilla".into();
    app.instance_browser.selected = "modded".into();
    app.page = Page::Instances;
    (root, app, ctx)
}

fn frame(ctx: &Context, app: &mut NsclApp, size: Vec2, events: Vec<Event>) -> FullOutput {
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| {
            crate::pages::instance_windows(ui.ctx(), app);
            show(ui, app);
        },
    );
    output.textures_delta.clear();
    output
}

fn click(ctx: &Context, app: &mut NsclApp, label: &str) {
    let size = Vec2::new(1100.0, 760.0);
    frame(ctx, app, size, vec![]).drop_without_applying_deltas();
    let output = frame(ctx, app, size, vec![]);
    let pos = text_rect(&output, label).center();
    output.drop_without_applying_deltas();
    press(ctx, app, size, pos);
}

/// The details tab bar and the file browser both label a folder-filled "模组" chip, so
/// this picks the last match, which is the one inside the file browser.
fn click_file_chip(ctx: &Context, app: &mut NsclApp, label: &str) {
    let size = Vec2::new(1100.0, 760.0);
    frame(ctx, app, size, vec![]).drop_without_applying_deltas();
    let output = frame(ctx, app, size, vec![]);
    let pos = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.galley.rect.translate(text.pos.to_vec2()))
            }
            _ => None,
        })
        .last()
        .unwrap_or_else(|| panic!("missing text: {label}"))
        .center();
    output.drop_without_applying_deltas();
    press(ctx, app, size, pos);
}

fn press(ctx: &Context, app: &mut NsclApp, size: Vec2, pos: Pos2) {
    frame(
        ctx,
        app,
        size,
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
    .drop_without_applying_deltas();
}

#[test]
fn browsing_filters_files_and_configuration_preserve_the_launch_selection() {
    let (_root, mut app, ctx) = populated();
    click(&ctx, &mut app, "实例设置");
    assert_eq!(app.instance_edit.as_ref().unwrap().id, "modded");
    assert_eq!(app.service.selected, "vanilla");
    app.instance_edit = None;
    click(&ctx, &mut app, "全部版本");
    click(&ctx, &mut app, "整合包");
    assert_eq!(app.instance_browser.selected, "pack-example");
    assert_eq!(app.service.selected, "vanilla");
    click(&ctx, &mut app, "文件");
    let output = frame(&ctx, &mut app, Vec2::new(1100.0, 760.0), vec![]);
    for label in ["存档", "资源包", "光影", "截图", "日志"] {
        text_rect(&output, label);
    }
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, "整合包");
    click(&ctx, &mut app, "全部版本");
    app.instance_browser.search = "Fabric world".into();
    let output = frame(&ctx, &mut app, Vec2::new(876.0, 430.0), vec![]);
    for label in ["返回开始", "新建实例", "安装新版本"] {
        let rect = text_rect(&output, label);
        assert!(
            rect.max.x < 876.0 && rect.max.y < 430.0,
            "{label}: {rect:?}"
        );
    }
    for label in ["存档", "日志"] {
        assert!(text_rect(&output, label).max.x < 876.0);
    }
    output.drop_without_applying_deltas();
    frame(
        &ctx,
        &mut app,
        Vec2::new(876.0, 430.0),
        vec![
            Event::PointerMoved(Pos2::new(700.0, 350.0)),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -300.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: Default::default(),
            },
        ],
    )
    .drop_without_applying_deltas();
    for _ in 0..6 {
        frame(&ctx, &mut app, Vec2::new(876.0, 430.0), vec![]).drop_without_applying_deltas();
    }
    let output = frame(&ctx, &mut app, Vec2::new(876.0, 430.0), vec![]);
    assert!(
        text_rect(&output, "日志").max.y < 430.0,
        "file shortcuts must be reachable by scrolling"
    );
    output.drop_without_applying_deltas();
    assert_eq!(app.instance_browser.selected, "modded");
    assert_eq!(app.service.selected, "vanilla");
}

#[test]
fn launch_stays_at_the_bottom_right_when_switching_and_scrolling_details() {
    let (_root, mut app, ctx) = populated();
    let size = Vec2::new(876.0, 430.0);
    let mut positions = Vec::new();
    for tab in [DetailTab::Overview, DetailTab::Files] {
        app.instance_browser.tab = tab;
        for events in [
            vec![],
            vec![
                Event::PointerMoved(Pos2::new(700.0, 230.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -1000.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: Default::default(),
                },
            ],
        ] {
            frame(&ctx, &mut app, size, events).drop_without_applying_deltas();
            let output = frame(&ctx, &mut app, size, vec![]);
            let launch = text_rect(&output, "启动游戏");
            assert!(launch.min.x > 700.0 && launch.min.y > 350.0, "{launch:?}");
            assert!(launch.max.x < size.x && launch.max.y < size.y, "{launch:?}");
            positions.push(launch);
            output.drop_without_applying_deltas();
        }
    }
    assert!(
        positions.iter().all(|rect| *rect == positions[0]),
        "{positions:?}"
    );
    assert_eq!(app.service.selected, "vanilla");
}

#[test]
fn file_categories_show_real_contents_details_and_missing_directory_feedback() {
    let (root, mut app, ctx) = populated();
    let mods = app
        .service
        .instances
        .iter()
        .find(|i| i.id == "modded")
        .unwrap()
        .game_directory(root.path())
        .unwrap()
        .join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::write(mods.join("local-example.jar"), [0; 2048]).unwrap();
    click(&ctx, &mut app, "文件");
    click_file_chip(&ctx, &mut app, "模组");
    wait_for_text(&ctx, &mut app, "local-example.jar");
    click(&ctx, &mut app, "local-example.jar");
    let output = frame(&ctx, &mut app, Vec2::new(1100.0, 760.0), vec![]);
    text_rect(&output, "文件 · 2.0 KiB");
    text_rect(&output, &mods.join("local-example.jar").to_string_lossy());
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, "截图");
    wait_for_text(&ctx, &mut app, "此目录尚未创建");
    assert!(!root.path().join("instances/modded/screenshots").exists());
    assert_eq!(app.service.selected, "vanilla");
}

fn wait_for_text(ctx: &Context, app: &mut NsclApp, label: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let output = frame(ctx, app, Vec2::new(1100.0, 760.0), vec![]);
        let found = output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == label));
        output.drop_without_applying_deltas();
        if found {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "missing text: {label}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn local_mod_links_open_the_correct_instances_file_details() {
    let (root, mut app, ctx) = populated();
    let instance = app
        .service
        .instances
        .iter()
        .find(|i| i.id == "modded")
        .unwrap();
    let installed = app
        .service
        .installed
        .iter()
        .find(|v| v.id == instance.version_id)
        .unwrap();
    let target = crate::content::Target::from_installed(instance, installed).unwrap();
    let mods = instance.game_directory(root.path()).unwrap().join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::write(mods.join("local-example.jar"), b"example").unwrap();
    app.instance_browser.tab = DetailTab::Mods;
    app.service.content.mode = ContentMode::Instance;
    app.service.content.target = instance.id.clone();
    app.service.content.managed = Some(crate::content::ManagedCatalog {
        target,
        entries: vec![],
        other_files: vec!["local-example.jar".into()],
    });
    wait_for_text(&ctx, &mut app, "local-example.jar");
    click(&ctx, &mut app, "local-example.jar");
    assert!(app.instance_browser.tab == DetailTab::Files);
    assert_eq!(app.instance_browser.selected, "modded");
    wait_for_text(&ctx, &mut app, "文件 · 7 B");
    assert_eq!(app.service.selected, "vanilla");
}

#[test]
fn archive_and_restore_keep_the_selected_instances_world_files() {
    let (root, mut app, ctx) = populated();
    app.instance_browser.selected = "pack-example".into();
    let instance = app
        .service
        .instances
        .iter()
        .find(|i| i.id == "pack-example")
        .unwrap()
        .clone();
    let world = instance
        .directory(root.path())
        .unwrap()
        .join("saves/world.dat");
    std::fs::create_dir_all(world.parent().unwrap()).unwrap();
    std::fs::write(&world, b"keep world").unwrap();
    click(&ctx, &mut app, "归档实例");
    assert_eq!(app.archive_confirm.as_ref().unwrap().id, "pack-example");
    click(&ctx, &mut app, "确认归档");
    assert!(app.service.instances.iter().all(|i| i.id != "pack-example"));
    click(&ctx, &mut app, "全部版本");
    click(&ctx, &mut app, "已归档");
    click(&ctx, &mut app, "恢复实例");
    assert_eq!(app.service.selected, "pack-example");
    assert_eq!(std::fs::read(world).unwrap(), b"keep world");
    assert!(app.service.error.is_none());
}

#[test]
fn new_instance_uses_existing_installation_and_the_default_isolation() {
    let (root, mut app, ctx) = populated();
    app.settings.default_isolation = instances::Isolation::Shared;
    click(&ctx, &mut app, "新建实例");
    app.new_instance_name = "Created from manager".into();
    app.new_instance_version = "1.20.1".into();
    click(&ctx, &mut app, "创建实例");
    assert!(!app.instance_browser.creating);
    let created = app
        .service
        .instances
        .iter()
        .find(|i| i.name == "Created from manager")
        .unwrap();
    assert_eq!(created.isolation, instances::Isolation::Shared);
    assert_eq!(created.game_directory(root.path()).unwrap(), root.path());
    assert_eq!(app.service.selected, created.id);
    assert!(app.service.error.is_none());
}

#[test]
fn mod_update_opens_the_existing_installation_plan_flow() {
    let (_root, mut app, ctx) = populated();
    let instance = app
        .service
        .instances
        .iter()
        .find(|i| i.id == "modded")
        .unwrap();
    let installed = app
        .service
        .installed
        .iter()
        .find(|v| v.id == instance.version_id)
        .unwrap();
    let target = crate::content::Target::from_installed(instance, installed).unwrap();
    let version: crate::content::ContentVersion = serde_json::from_value(serde_json::json!({
        "id":"v2", "project_id":"example", "name":"Example mod", "version_number":"2.0",
        "version_type":"release", "date_published":"2026-01-01", "game_versions":["1.20.1"], "loaders":["fabric"], "files":[]
    })).unwrap();
    app.service.content.kind = crate::content::ContentKind::Mod;
    app.service.content.mode = ContentMode::Instance;
    app.service.content.target = "modded".into();
    app.instance_browser.tab = DetailTab::Mods;
    app.service.content.managed = Some(crate::content::ManagedCatalog {
        target,
        entries: vec![crate::content::ManagedEntry {
            installed: crate::content::ManagedMod {
                version: version.clone(),
                file: crate::content::InstalledFile {
                    path: "mods/example.jar".into(),
                    hashes: Default::default(),
                    size: 1,
                },
            },
            intact: true,
        }],
        other_files: vec![],
    });
    app.service.content.updates.available = vec![crate::content::UpdateCandidate {
        project_id: "example".into(),
        current_version: "1.0".into(),
        available: version,
    }];
    // Fail before HTTP so this interaction test does not depend on external services.
    app.settings.source.kind = crate::minecraft::SourceKind::Custom;
    app.settings.source.custom_base = "invalid".into();
    click(&ctx, &mut app, "查看更新方案");
    // Updating an installed mod must stay inside the instance that owns it.
    assert!(app.page == Page::Instances);
    assert_eq!(app.instance_browser.tab, DetailTab::Mods);
    assert!(app.service.busy());
    assert_eq!(app.service.selected, "vanilla");
}
