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

#[test]
fn pack_export_dialog_opens_for_the_browsed_instance_with_safe_defaults() {
    let (_root, mut app, ctx) = populated();
    click(&ctx, &mut app, "导出整合包");
    assert!(app.instance_browser.export.is_some());
    let output = frame(&ctx, &mut app, Vec2::new(960.0, 720.0), vec![]);
    for label in [
        "来源实例：Fabric world",
        "包含存档",
        "包含游戏设置与服务器列表",
        "保存为新文件（.mrpack）",
        "开始导出",
    ] {
        text_rect(&output, label);
    }
    assert_eq!(app.service.selected, "vanilla");
    assert!(!app.service.busy());
    output.drop_without_applying_deltas();
}

#[test]
fn pack_export_service_uses_the_browsed_instance_and_reports_saved_path() {
    let (root, mut app, ctx) = populated();
    let instance = app
        .service
        .instances
        .iter()
        .find(|i| i.id == "modded")
        .unwrap()
        .clone();
    let directory = instance.game_directory(root.path()).unwrap();
    std::fs::create_dir_all(directory.join("config")).unwrap();
    std::fs::write(directory.join("config/export.txt"), b"test").unwrap();
    let output = root.path().join("service.mrpack");
    let options = crate::content::PackExportOptions::for_instance(&instance);
    app.service.task = crate::service::Task::Content;
    app.service.export_pack(
        &instance,
        options.clone(),
        output.clone(),
        &app.settings,
        &ctx,
    );
    assert!(!output.exists());
    app.service.task = crate::service::Task::Idle;
    app.service.game = Some((1, root.path().join("game.log")));
    app.service.export_pack(
        &instance,
        options.clone(),
        output.clone(),
        &app.settings,
        &ctx,
    );
    assert!(!app.service.busy());
    app.service.game = None;
    app.service
        .export_pack(&instance, options, output.clone(), &app.settings, &ctx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.service.busy() && std::time::Instant::now() < deadline {
        app.service.poll(&app.settings);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!app.service.busy());
    assert!(app.service.error.is_none(), "{:?}", app.service.error);
    assert_eq!(app.service.exported_pack.as_ref(), Some(&output));
    assert_eq!(app.service.selected, "vanilla");
    let plan = crate::content::inspect_pack(root.path(), &output).unwrap();
    assert_eq!(plan.loader.as_ref().unwrap().kind, LoaderKind::Fabric);
    assert_eq!(plan.override_count(), 1);
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
        .next_back()
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
    crate::service::tests::seed_local_mod_catalog(&mut app);
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
    crate::service::tests::seed_local_mod_catalog(&mut app);
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

fn removal_fixture() -> (
    tempfile::TempDir,
    NsclApp,
    Context,
    crate::content::RemovalPlan,
    PathBuf,
) {
    use sha1::{Digest, Sha1};
    use sha2::Sha512;
    let (root, app, ctx) = populated();
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
    let directory = instance.game_directory(root.path()).unwrap();
    std::fs::create_dir_all(directory.join("mods")).unwrap();
    let file = directory.join("mods/example.jar");
    std::fs::write(&file, b"keep this mod").unwrap();
    let version = serde_json::from_value(serde_json::json!({"id":"v1","project_id":"example","name":"Example mod","version_number":"1.0","version_type":"release","date_published":"2026-01-01","game_versions":["1.20.1"],"loaders":["fabric"],"files":[]})).unwrap();
    crate::store::write_json(
        &directory.join(".nscl-content.json"),
        &crate::content::Registry {
            mods: std::collections::BTreeMap::from([(
                "example".into(),
                crate::content::ManagedMod {
                    version,
                    file: crate::content::InstalledFile {
                        path: "mods/example.jar".into(),
                        hashes: std::collections::BTreeMap::from([
                            (
                                "sha1".into(),
                                format!("{:x}", Sha1::digest(b"keep this mod")),
                            ),
                            (
                                "sha512".into(),
                                format!("{:x}", Sha512::digest(b"keep this mod")),
                            ),
                        ]),
                        size: 13,
                    },
                },
            )]),
            pack: None,
        },
    )
    .unwrap();
    let plan = crate::content::plan_removal(
        root.path(),
        &target,
        &std::collections::BTreeSet::from(["example".into()]),
    )
    .unwrap();
    (root, app, ctx, plan, file)
}

#[test]
fn mod_removal_plan_never_becomes_a_pack_installation_when_switching_pages() {
    let (_root, mut app, ctx, plan, file) = removal_fixture();
    app.service.content.mode = ContentMode::Instance;
    app.service.content.target = "modded".into();
    app.service.content.plan = Some(crate::service::ContentPlan::Remove(plan));
    app.category = 1;
    app.page = Page::Downloads;
    for task in [crate::service::Task::Content, crate::service::Task::Idle] {
        app.service.task = task;
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 760.0))),
                ..Default::default()
            },
            |ui| crate::pages::downloads(ui, &mut app),
        );
        output.textures_delta.clear();
        assert!(!output.shapes.iter().any(
            |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == "安装整合包")
        ));
        text_rect(&output, "搜索");
        output.drop_without_applying_deltas();
    }
    assert!(app.service.content.plan.is_none());
    assert_eq!(app.service.content.mode, ContentMode::Download);
    assert!(app.service.content.target.is_empty());
    assert_eq!(app.service.selected, "vanilla");
    assert_eq!(std::fs::read(file).unwrap(), b"keep this mod");
}

#[test]
fn executing_a_plan_from_another_surface_or_instance_is_rejected() {
    let (_root, mut app, ctx, plan, file) = removal_fixture();
    app.service.content.plan = Some(crate::service::ContentPlan::Remove(plan));
    for (mode, kind, target) in [
        (
            ContentMode::Download,
            crate::content::ContentKind::Modpack,
            "",
        ),
        (
            ContentMode::Instance,
            crate::content::ContentKind::Mod,
            "pack-example",
        ),
    ] {
        app.service.content.mode = mode;
        app.service.content.kind = kind;
        app.service.content.target = target.into();
        app.service.error = None;
        app.service.content_install(&app.settings, &ctx);
        assert!(!app.service.busy());
        assert!(app.service.error.as_ref().unwrap().contains("方案不属于"));
        assert_eq!(std::fs::read(&file).unwrap(), b"keep this mod");
    }
    // Even a valid removal context must never run through the pack-install action.
    app.service.content.mode = ContentMode::Instance;
    app.service.content.target = "modded".into();
    app.service.error = None;
    app.service.content_install_pack(&app.settings, &ctx);
    assert!(!app.service.busy());
    assert!(
        app.service
            .error
            .as_ref()
            .unwrap()
            .contains("不是下载页的整合包")
    );
    assert_eq!(std::fs::read(file).unwrap(), b"keep this mod");
}

#[test]
fn executing_a_removal_plan_in_its_own_instance_still_works() {
    let (root, mut app, ctx, plan, file) = removal_fixture();
    app.service.content_instance("modded");
    app.service.content.plan = Some(crate::service::ContentPlan::Remove(plan));
    app.settings.source.kind = crate::minecraft::SourceKind::Official;
    app.service.content_install(&app.settings, &ctx);
    assert!(app.service.busy());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.service.busy() && std::time::Instant::now() < deadline {
        app.service.poll(&app.settings);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!app.service.busy());
    assert!(app.service.error.is_none(), "{:?}", app.service.error);
    assert!(!file.exists());
    let directory = root.path().join("instances/modded");
    assert!(
        crate::content::registry(&directory)
            .unwrap()
            .mods
            .is_empty()
    );
}

#[test]
fn instance_search_exposes_paging_without_binding_the_launch_selection() {
    let (_root, mut app, ctx) = populated();
    app.instance_browser.tab = DetailTab::Mods;
    app.service.content_instance("modded");
    app.service.content.adding = true;
    app.service.content.results = crate::content::SearchPage {
        hits: vec![crate::content::Project {
            project_id: "example".into(),
            title: "Example mod".into(),
            description: String::new(),
            author: String::new(),
            downloads: 1,
            icon_url: None,
        }],
        offset: 0,
        total_hits: 45,
    };
    frame(&ctx, &mut app, Vec2::new(1100.0, 1000.0), vec![]).drop_without_applying_deltas();
    let output = frame(&ctx, &mut app, Vec2::new(1100.0, 1000.0), vec![]);
    text_rect(&output, "上一页");
    text_rect(&output, "下一页");
    text_rect(&output, "45 条结果 · 第 1 页");
    output.drop_without_applying_deltas();
    assert!(!app.service.busy());
    assert_eq!(app.service.content.target, "modded");
    assert_eq!(app.service.selected, "vanilla");
}

fn settle_local(
    app: &mut NsclApp,
    ctx: &Context,
    predicate: impl Fn(&crate::content::LocalCatalog) -> bool,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        app.service.poll(&app.settings);
        frame(ctx, app, Vec2::new(1100.0, 760.0), vec![]).drop_without_applying_deltas();
        if !app.service.busy()
            && app
                .service
                .local
                .catalog
                .as_ref()
                .is_some_and(|c| predicate(c))
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "resource task timed out: {:?}",
            app.service.error
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn resource_pack_management_imports_toggles_and_recycles_in_the_selected_instance() {
    use std::io::Write;
    let (root, mut app, ctx) = populated();
    let source = root.path().join("Example Pack.zip");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&source).unwrap());
    archive
        .start_file("pack.mcmeta", zip::write::SimpleFileOptions::default())
        .unwrap();
    archive
        .write_all(br#"{"pack":{"pack_format":15,"description":"Example"}}"#)
        .unwrap();
    archive.finish().unwrap();
    app.instance_browser.tab = DetailTab::ResourcePacks;
    let instance = app
        .service
        .instances
        .iter()
        .find(|i| i.id == "modded")
        .unwrap()
        .clone();
    app.settings.source.kind = crate::minecraft::SourceKind::Custom;
    app.settings.source.custom_base = "invalid".into();
    app.service.local_import(
        &instance,
        crate::content::LocalKind::ResourcePacks,
        Some(vec![source.clone()]),
        &app.settings,
        &ctx,
    );
    settle_local(&mut app, &ctx, |c| {
        c.entries.len() == 1 && c.entries[0].enabled
    });
    click(&ctx, &mut app, "禁用");
    settle_local(&mut app, &ctx, |c| {
        c.entries.len() == 1 && !c.entries[0].enabled
    });
    assert!(
        !root
            .path()
            .join("instances/modded/resourcepacks/Example Pack.zip")
            .exists()
    );
    click(&ctx, &mut app, "启用");
    settle_local(&mut app, &ctx, |c| {
        c.entries.len() == 1 && c.entries[0].enabled
    });
    click(&ctx, &mut app, "移除");
    assert!(app.service.local.remove.is_some());
    click(&ctx, &mut app, "确认移除");
    settle_local(&mut app, &ctx, |c| c.entries.is_empty());
    let trash = root
        .path()
        .join("instances/modded/.nscl/trash/resourcepacks");
    let directory = std::fs::read_dir(trash)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        std::fs::read(directory.join("Example Pack.zip")).unwrap(),
        std::fs::read(source).unwrap()
    );
    assert_eq!(app.service.selected, "vanilla");
}

#[test]
fn importing_resources_is_blocked_while_a_game_or_another_task_is_running() {
    let (_root, mut app, ctx) = populated();
    let instance = app
        .service
        .instances
        .iter()
        .find(|i| i.id == "modded")
        .unwrap()
        .clone();
    app.service.game = Some((123, PathBuf::from("game.log")));
    app.service.local_import(
        &instance,
        crate::content::LocalKind::Shaders,
        Some(vec![PathBuf::from("absent.zip")]),
        &app.settings,
        &ctx,
    );
    assert!(!app.service.busy());
    app.service.game = None;
    app.service.task = crate::service::Task::Content;
    app.service.local_import(
        &instance,
        crate::content::LocalKind::Shaders,
        None,
        &app.settings,
        &ctx,
    );
    assert!(app.service.local.key.is_none());
}
