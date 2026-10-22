use super::*;
use crate::{app::NsclApp, pages};
use egui::{Context, FullOutput, Pos2, RawInput, Rect, Vec2};

pub(super) fn fixture() -> (tempfile::TempDir, NsclApp, Context) {
    let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    std::fs::create_dir_all(&parent).unwrap();
    let root = tempfile::tempdir_in(parent).unwrap();
    let mut app = NsclApp::new();
    app.project_icons = crate::project_icons::ProjectIcons::new(root.path().join("icons"));
    app.settings.game_directory = root.path().to_string_lossy().into_owned();
    app.service = GameService::new(&app.settings);
    app.service.versions_requested = true;
    app.service.versions = vec![VersionEntry {
        id: "1.20.1".into(),
        kind: "release".into(),
        release_time: "2023-06-12T00:00:00Z".into(),
        url: String::new(),
        sha1: String::new(),
    }];
    app.service.loaders_requested = Some(("1.20.1".into(), LoaderKind::Forge));
    app.service.loaders = vec![LoaderVersion {
        minecraft: "1.20.1".into(),
        kind: LoaderKind::Forge,
        version: "47.4.28".into(),
        stable: true,
    }];
    let ctx = Context::default();
    crate::install_cjk_fonts(&ctx);
    crate::theme::apply(&ctx);
    (root, app, ctx)
}

pub(super) fn frame(
    ctx: &Context,
    app: &mut NsclApp,
    size: Vec2,
    events: Vec<egui::Event>,
) -> FullOutput {
    ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| pages::downloads(ui, app),
    )
}

pub(super) fn text_rect(output: &FullOutput, label: &str) -> Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.galley.rect.translate(text.pos.to_vec2()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing text: {label}"))
}

fn click(ctx: &Context, app: &mut NsclApp, size: Vec2, pos: Pos2) {
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
fn selecting_minecraft_and_loader_waits_for_install_confirmation() {
    let (root, mut app, ctx) = fixture();
    let size = Vec2::new(1116.0, 510.0);
    let output = frame(&ctx, &mut app, size, vec![]);
    let pos = text_rect(&output, "选择版本").center();
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, size, pos);
    assert_eq!(app.download.version.as_ref().unwrap().id, "1.20.1");
    assert!(!app.service.busy());

    let output = frame(&ctx, &mut app, size, vec![]);
    let pos = text_rect(&output, "Forge").center();
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, size, pos);
    assert!(app.download.target().is_none());
    let output = frame(&ctx, &mut app, size, vec![]);
    let install_pos = text_rect(&output, "开始安装").center();
    click(&ctx, &mut app, size, install_pos);
    assert!(
        !app.service.busy(),
        "a loader version is required before installation"
    );
    let pos = text_rect(&output, "选择").center();
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, size, pos);
    assert!(app.download.loader.is_some());
    assert!(
        !app.service.busy(),
        "choosing a loader must not start a download"
    );

    app.service.installed.push(InstalledVersion {
        id: "1.20.1".into(),
        kind: "release".into(),
        java_major: 17,
        platform: String::new(),
        metadata_sha1: String::new(),
        loader: None,
        extra_files: vec![],
    });
    let output = frame(&ctx, &mut app, size, vec![]);
    text_rect(&output, "开始安装");
    output.drop_without_applying_deltas();
    let loader = app.download.loader.clone().unwrap();
    app.service.installed[0].id = loader.id().unwrap();
    app.service.installed[0].loader = Some(loader);
    let output = frame(&ctx, &mut app, size, vec![]);
    text_rect(&output, "校验 / 修复");
    output.drop_without_applying_deltas();

    let output = frame(&ctx, &mut app, size, vec![]);
    let pos = text_rect(&output, "不安装加载器").center();
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, size, pos);
    assert!(app.download.loader.is_none());
    let output = frame(&ctx, &mut app, size, vec![]);
    let pos = text_rect(&output, "返回版本列表").center();
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, size, pos);
    assert!(app.download.version.is_none());
    assert!(!app.service.busy());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn installation_controls_fit_the_minimum_window_during_query_and_install() {
    let (_root, mut app, ctx) = fixture();
    app.download.choose_version(app.service.versions[0].clone());
    app.download.choose_kind(Some(LoaderKind::Forge));
    // Available download-card area inside the launcher's 960 x 640 minimum window.
    let size = Vec2::new(876.0, 390.0);
    for task in [Task::Idle, Task::LoaderVersions, Task::Install] {
        app.service.task = task;
        if task == Task::LoaderVersions {
            app.service.loaders.clear();
        }
        let output = frame(&ctx, &mut app, size, vec![]);
        let install = text_rect(&output, "开始安装");
        assert!(
            install.max.y < size.y && install.max.x < size.x,
            "install button is outside the window: {install:?}"
        );
        if task == Task::LoaderVersions {
            let cancel = text_rect(&output, "取消加载");
            assert!(
                cancel.max.y < install.min.y,
                "query controls overlap the footer"
            );
        }
        if task == Task::Install {
            let cancel = text_rect(&output, "取消任务");
            assert!(
                cancel.max.y < size.y,
                "install progress is outside the window: {cancel:?}"
            );
        }
        output.drop_without_applying_deltas();
    }
}

#[test]
fn resource_catalog_and_versions_fit_small_windows_without_starting_tasks() {
    let (_root, mut app, ctx) = fixture();
    let size = Vec2::new(876.0, 390.0);
    for category in [1, 2] {
        app.category = category;
        app.service.task = Task::Idle;
        app.service.content_kind(if category == 1 {
            crate::content::ContentKind::Modpack
        } else {
            crate::content::ContentKind::Mod
        });
        app.service.content.results.hits = vec![crate::content::Project {
            project_id: "example".into(),
            title: "Test resource".into(),
            description: "Example description".into(),
            author: "Author".into(),
            downloads: 1,
            icon_url: None,
        }];
        for task in [Task::Idle, Task::Content] {
            app.service.task = task;
            let mut output = frame(&ctx, &mut app, size, vec![]);
            output.textures_delta.clear();
            let search = text_rect(&output, "搜索");
            let next = text_rect(&output, "下一页");
            assert!(search.max.x < size.x && next.max.y < size.y);
            if category == 2 {
                assert!(text_rect(&output, "管理已装模组").max.x < size.x);
            }
            if task == Task::Idle {
                let title = text_rect(&output, "Test resource");
                let placeholder = text_rect(&output, "T");
                let action = text_rect(&output, "查看版本");
                assert!(placeholder.max.x < title.min.x);
                assert!(title.max.x < action.min.x && action.max.x < size.x);
            }
            if task == Task::Content {
                assert!(text_rect(&output, "取消任务").max.y < size.y);
            }
            output.drop_without_applying_deltas();
        }
    }
    app.service.task = Task::Idle;
    app.service.content.project = Some(crate::content::Project {
        project_id: "example".into(),
        title: "Test resource".into(),
        description: String::new(),
        author: String::new(),
        downloads: 1,
        icon_url: None,
    });
    app.service.content.versions=vec![serde_json::from_value(serde_json::json!({"id":"v1","project_id":"example","name":"Test version","version_number":"1","version_type":"release","date_published":"2026-01-01","game_versions":["1.20.1"],"loaders":["fabric"],"files":[]})).unwrap()];
    let output = frame(&ctx, &mut app, size, vec![]);
    assert!(text_rect(&output, "查看方案").max.y < size.y);
    assert!(app.service.task == Task::Idle);
    output.drop_without_applying_deltas();
}

#[test]
fn local_pack_preview_waits_for_confirmation_and_optional_files_are_a_choice() {
    use std::io::Write;
    let (root, mut app, ctx) = fixture();
    let size = Vec2::new(876.0, 390.0);
    let path = root.path().join("preview.mrpack");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file(
        "modrinth.index.json",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    let metadata = serde_json::json!({"formatVersion":1,"game":"minecraft","versionId":"1","name":"Preview pack","dependencies":{"minecraft":"1.20.1"},"files":[{"path":"config/optional.txt","hashes":{"sha1":"0".repeat(40),"sha512":"0".repeat(128)},"fileSize":1,"downloads":["https://cdn.modrinth.com/example"],"env":{"client":"optional","server":"unsupported"}}]});
    zip.write_all(&serde_json::to_vec(&metadata).unwrap())
        .unwrap();
    zip.finish().unwrap();
    app.category = 1;
    app.service.content.plan = Some(super::ContentPlan::Pack(
        crate::content::inspect_pack(root.path(), &path).unwrap(),
    ));
    let output = frame(&ctx, &mut app, size, vec![]);
    assert!(text_rect(&output, "安装整合包").max.y < size.y);
    let pos = text_rect(&output, "包含 1 个可选客户端文件").center();
    output.drop_without_applying_deltas();
    click(&ctx, &mut app, size, pos);
    let Some(super::ContentPlan::Pack(plan)) = &app.service.content.plan else {
        panic!("preview closed");
    };
    assert!(plan.include_optional);
    assert_eq!(plan.download_count(), 1);
    assert!(app.service.task == Task::Idle);
    assert!(!root.path().join("instances").exists());
}
