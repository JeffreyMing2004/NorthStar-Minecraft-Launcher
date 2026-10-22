use super::*;
use crate::app::Page;
use egui::{DroppedFile, RawInput};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug)]
struct FileDrop(PathBuf);
impl DroppedFile for FileDrop {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}
fn drop_files(app: &mut crate::app::NsclApp, ctx: &egui::Context, files: &[PathBuf]) {
    ctx.run_ui(
        RawInput {
            dropped_files: files
                .iter()
                .map(|p| Arc::new(FileDrop(p.clone())) as egui::DroppedFileHandle)
                .collect(),
            ..Default::default()
        },
        |_| {
            app.handle_pack_drop(ctx);
            app.handle_pack_drop(ctx); // Same input on a second layout pass must not submit twice.
        },
    )
    .drop_without_applying_deltas();
}
fn archive(path: &Path, entry: &str, bytes: &[u8]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    zip.start_file(entry, zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap();
}
fn finish(app: &mut crate::app::NsclApp) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.service.busy() && Instant::now() < deadline {
        app.service.poll(&app.settings);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.service.busy(), "drop task timed out");
}

#[test]
fn home_drop_installs_a_detected_modrinth_zip_without_confirmation() {
    use serde_json::json;
    use sha1::{Digest, Sha1};
    let (root, mut app, ctx) = super::tests::fixture();
    app.page = Page::Home;
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr());
    let sha1 = |bytes: &[u8]| format!("{:x}", Sha1::digest(bytes));
    let client = b"test game client".to_vec();
    let index = serde_json::to_vec(&json!({"objects":{}})).unwrap();
    let metadata = serde_json::to_vec(&json!({"id":"test-drop","type":"release","mainClass":"test.Main","downloads":{"client":{"url":format!("{base}/client.jar"),"sha1":sha1(&client),"size":client.len()}},"assetIndex":{"id":"test-assets","url":format!("{base}/index.json"),"sha1":sha1(&index),"size":index.len()},"libraries":[],"arguments":{"jvm":[],"game":[]},"javaVersion":{"majorVersion":17}})).unwrap();
    let manifest = serde_json::to_vec(&json!({"versions":[{"id":"test-drop","type":"release","url":format!("{base}/version.json"),"sha1":sha1(&metadata)}]})).unwrap();
    let routes = std::collections::BTreeMap::from([
        ("/mc/game/version_manifest_v2.json", manifest),
        ("/version.json", metadata),
        ("/client.jar", client),
        ("/index.json", index),
    ]);
    let worker = std::thread::spawn(move || {
        for _ in 0..4 {
            let request = server
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .expect("missing install request");
            let data = routes
                .get(request.url())
                .expect("unexpected install request");
            request
                .respond(tiny_http::Response::from_data(data.clone()))
                .unwrap();
        }
    });
    app.settings.source = minecraft::Source {
        kind: minecraft::SourceKind::Custom,
        custom_base: base,
        fallback_to_official: false,
    };
    let path = root.path().join("dropped.zip");
    archive(&path, "modrinth.index.json", &serde_json::to_vec(&json!({"formatVersion":1,"game":"minecraft","versionId":"1","name":"Dropped pack","dependencies":{"minecraft":"test-drop"},"files":[]})).unwrap());
    drop_files(&mut app, &ctx, &[path]);
    assert!(app.service.busy());
    assert!(app.service.error.is_none());
    assert!(app.service.content.plan.is_none());
    finish(&mut app);
    worker.join().unwrap();
    assert!(app.service.error.is_none(), "{:?}", app.service.error);
    assert!(app.page == Page::Home);
    let instance = app
        .service
        .instances
        .iter()
        .find(|i| i.name == "Dropped pack 1")
        .unwrap();
    assert!(instance.directory(root.path()).unwrap().is_dir());
    assert_eq!(instance.version_id, "test-drop");
}

#[test]
fn drops_reject_busy_multiple_or_unsupported_packs_without_publishing_an_instance() {
    let (root, mut app, ctx) = super::tests::fixture();
    app.page = Page::Home;
    let path = root.path().join("curseforge.zip");
    archive(&path, "manifest.json", b"{}");
    app.service.task = Task::Content;
    drop_files(&mut app, &ctx, std::slice::from_ref(&path));
    assert!(app.service.error.as_ref().unwrap().contains("当前任务"));
    app.service.task = Task::Idle;
    drop_files(&mut app, &ctx, &[path.clone(), path.clone()]);
    assert!(app.service.error.as_ref().unwrap().contains("一个整合包"));
    drop_files(&mut app, &ctx, std::slice::from_ref(&path));
    finish(&mut app);
    assert!(app.service.error.as_ref().unwrap().contains("CurseForge"));
    assert!(app.service.instances.is_empty());
    assert!(!root.path().join("instances").exists());
}
