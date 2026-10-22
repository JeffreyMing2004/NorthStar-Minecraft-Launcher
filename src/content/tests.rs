use super::*;
use crate::{
    instances,
    minecraft::{Downloader, InstalledVersion, LoaderKind, LoaderVersion, Source, SourceKind},
};
use serde_json::json;
use sha1::{Digest, Sha1};
use sha2::Sha512;
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[test]
fn project_logos_allow_missing_null_and_empty_urls() {
    let mut project = json!({"project_id":"test","title":"Test","description":"","downloads":1});
    assert!(
        serde_json::from_value::<Project>(project.clone())
            .unwrap()
            .icon_url
            .is_none()
    );
    for url in [
        serde_json::Value::Null,
        json!(""),
        json!("https://cdn.modrinth.com/data/test/icon.png"),
    ] {
        project["icon_url"] = url.clone();
        let parsed: Project = serde_json::from_value(project.clone()).unwrap();
        assert_eq!(parsed.icon_url.as_deref(), url.as_str());
    }
}

struct Server {
    base: String,
    routes: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr());
        let routes = Arc::new(Mutex::new(BTreeMap::<String, Vec<u8>>::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (map, seen, flag) = (routes.clone(), requests.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                if let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(50)) {
                    seen.lock().unwrap().push(request.url().to_owned());
                    let data = map
                        .lock()
                        .unwrap()
                        .get(request.url().split('?').next().unwrap())
                        .cloned();
                    let response = match data {
                        Some(bytes) => tiny_http::Response::from_data(bytes),
                        None => tiny_http::Response::from_data(b"not found".to_vec())
                            .with_status_code(404),
                    };
                    let _ = request.respond(response);
                }
            }
        });
        Self {
            base,
            routes,
            requests,
            stop,
            thread: Some(thread),
        }
    }
    fn put(&self, path: &str, bytes: Vec<u8>) {
        self.routes.lock().unwrap().insert(path.into(), bytes);
    }
    fn json(&self, path: &str, value: serde_json::Value) {
        self.put(path, serde_json::to_vec(&value).unwrap());
    }
    fn downloader(&self) -> Downloader {
        Downloader::new(
            Source {
                kind: SourceKind::Custom,
                custom_base: self.base.clone(),
                fallback_to_official: false,
            },
            Arc::new(AtomicBool::new(false)),
            Arc::new(|_| {}),
        )
        .unwrap()
    }
    fn api(&self) -> Modrinth {
        Modrinth::with_base(self.downloader(), format!("{}/api", self.base))
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}

fn temporary() -> tempfile::TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&parent).unwrap();
    tempfile::tempdir_in(parent).unwrap()
}
fn hashes(bytes: &[u8]) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("sha1".into(), format!("{:x}", Sha1::digest(bytes))),
        ("sha512".into(), format!("{:x}", Sha512::digest(bytes))),
    ])
}
fn version(
    server: &Server,
    project: &str,
    id: &str,
    dependencies: serde_json::Value,
) -> ContentVersion {
    let bytes = format!("jar for {id}").into_bytes();
    server.put(&format!("/files/{id}.jar"), bytes.clone());
    let value = json!({"id":id,"project_id":project,"name":project,"version_number":id,"version_type":"release","date_published":"2026-01-01T00:00:00Z","game_versions":["test-1"],"loaders":["fabric"],"dependencies":dependencies,"files":[{"filename":format!("{project}-{id}.jar"),"url":format!("{}/files/{id}.jar",server.base),"hashes":hashes(&bytes),"size":bytes.len(),"primary":true}]});
    server.json(
        &format!("/api/project/{project}"),
        json!({"project_type":"mod","client_side":"required"}),
    );
    server.json(&format!("/api/version/{id}"), value.clone());
    server.json(
        &format!("/api/project/{project}/version"),
        json!([value.clone()]),
    );
    serde_json::from_value(value).unwrap()
}
fn target(root: &Path) -> Target {
    let loader = LoaderVersion {
        kind: LoaderKind::Fabric,
        minecraft: "test-1".into(),
        version: "0.20.1".into(),
        stable: true,
    };
    let id = loader.id().unwrap();
    let directory = root.join("versions").join(&id);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join(format!("{id}.json")), b"{}").unwrap();
    crate::store::write_json(
        &directory.join("nscl-instance.json"),
        &InstalledVersion {
            id: id.clone(),
            kind: "fabric".into(),
            java_major: 17,
            platform: String::new(),
            metadata_sha1: String::new(),
            loader: Some(loader),
            extra_files: vec![],
        },
    )
    .unwrap();
    let instance = instances::create(root, &id, "Test instance").unwrap();
    Target::from_instance(root, &instance).unwrap()
}

#[test]
fn mod_dependencies_are_installed_together_and_updates_preserve_saves() {
    let root = temporary();
    let server = Server::new();
    let api = server.api();
    let target = target(root.path());
    let dep = version(&server, "dependency", "dep1", json!([]));
    let first = version(
        &server,
        "example",
        "v1",
        json!([{"project_id":"dependency","version_id":"dep1","dependency_type":"required"}]),
    );
    let directory = target.instance.directory(root.path()).unwrap();
    fs::create_dir_all(directory.join("saves/world")).unwrap();
    fs::write(directory.join("saves/world/level.dat"), b"keep world").unwrap();
    let plan = plan_mod(root.path(), target.clone(), first, &api).unwrap();
    assert_eq!(plan.versions.len(), 2);
    assert!(!directory.join("mods").exists());
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    assert_eq!(registry(&directory).unwrap().mods.len(), 2);
    assert!(
        directory
            .join(format!(
                "mods/{}",
                dep.file(ContentKind::Mod).unwrap().filename
            ))
            .is_file()
    );
    let second = version(
        &server,
        "example",
        "v2",
        json!([{"project_id":"dependency","dependency_type":"required"}]),
    );
    let plan = plan_mod(root.path(), target.clone(), second, &api).unwrap();
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    assert!(!directory.join("mods/example-v1.jar").exists());
    assert!(directory.join("mods/example-v2.jar").exists());
    assert_eq!(
        fs::read(directory.join("saves/world/level.dat")).unwrap(),
        b"keep world"
    );
    assert!(
        check_instance(root.path(), &target.instance, &api.downloader)
            .unwrap()
            .errors
            .is_empty()
    );
}

#[test]
fn version_loader_dependency_and_server_only_conflicts_are_blocked() {
    for failure in [
        "minecraft",
        "loader",
        "dependency",
        "server",
        "incompatible",
    ] {
        let root = temporary();
        let server = Server::new();
        let target = target(root.path());
        let mut selected = version(&server, "example", "v1", json!([]));
        match failure {
            "minecraft" => selected.game_versions = vec!["other".into()],
            "loader" => selected.loaders = vec!["neoforge".into()],
            "dependency" => {
                selected.dependencies = serde_json::from_value(
                    json!([{"project_id":"missing","dependency_type":"required"}]),
                )
                .unwrap();
            }
            "server" => server.json(
                "/api/project/example",
                json!({"project_type":"mod","client_side":"unsupported"}),
            ),
            _ => {
                selected.dependencies = serde_json::from_value(
                    json!([{"project_id":"example","dependency_type":"incompatible"}]),
                )
                .unwrap();
            }
        }
        assert!(
            plan_mod(root.path(), target, selected, &server.api()).is_err(),
            "{failure}"
        );
    }
}

#[test]
fn cancellation_hash_failure_and_unmanaged_filename_never_overwrite_mods() {
    for failure in ["cancel", "hash", "collision"] {
        let root = temporary();
        let server = Server::new();
        let api = server.api();
        let target = target(root.path());
        let selected = version(&server, "example", "v1", json!([]));
        let directory = target.instance.directory(root.path()).unwrap();
        let plan = plan_mod(root.path(), target.clone(), selected, &api).unwrap();
        if failure == "cancel" {
            api.downloader.cancel.store(true, Ordering::Relaxed);
        }
        if failure == "hash" {
            server.put("/files/v1.jar", b"bad bytes".to_vec());
        }
        if failure == "collision" {
            fs::create_dir_all(directory.join("mods")).unwrap();
            fs::write(directory.join("mods/example-v1.jar"), b"manual mod").unwrap();
        }
        assert!(install_mods(root.path(), &plan, &api.downloader).is_err());
        assert!(!directory.join(REGISTRY).exists());
        if failure == "collision" {
            assert_eq!(
                fs::read(directory.join("mods/example-v1.jar")).unwrap(),
                b"manual mod"
            );
        }
    }
}

#[test]
fn safe_paths_and_hashes_reject_traversal_and_windows_aliases() {
    let root = temporary();
    for value in [
        "../outside.jar",
        "mods/../../file",
        "C:/file",
        "mods\\file.jar",
        "mods/CON.jar",
        "mods/x.jar:stream",
        "mods/name. ",
    ] {
        assert!(files::safe_path(root.path(), value).is_err(), "{value}");
    }
    let file = root.path().join("file");
    fs::write(&file, b"hello").unwrap();
    let mut expected = hashes(b"hello");
    expected.insert("sha512".into(), "0".repeat(128));
    assert!(!files::verified(&file, &expected, 5).unwrap());
}

#[test]
fn case_aliases_cannot_create_duplicate_mods_or_overwrite_manual_files() {
    let root = temporary();
    let server = Server::new();
    let api = server.api();
    let target = target(root.path());
    let selected = version(&server, "example", "v1", json!([]));
    let plan = plan_mod(root.path(), target.clone(), selected, &api).unwrap();
    let directory = target.instance.directory(root.path()).unwrap();
    fs::create_dir_all(directory.join("mods")).unwrap();
    let existing = directory.join("mods/EXAMPLE-V1.JAR");
    fs::write(&existing, b"manual file").unwrap();
    let error = install_mods(root.path(), &plan, &api.downloader).unwrap_err();
    assert!(error.contains("大小写"));
    assert_eq!(fs::read(existing).unwrap(), b"manual file");
    assert!(!directory.join(REGISTRY).exists());
}

#[test]
fn search_encodes_filters_and_does_not_trust_unfiltered_api_versions() {
    let root = temporary();
    let server = Server::new();
    let target = target(root.path());
    let api = server.api();
    let good = version(&server, "example", "v1", json!([]));
    let mut wrong = good.clone();
    wrong.id = "wrong".into();
    wrong.loaders = vec!["forge".into()];
    server.json("/api/project/example/version", json!([wrong, good]));
    server.json(
        "/api/search",
        json!({"hits":[],"offset":20,"total_hits":40}),
    );
    let query = "sodium & 汉字";
    assert_eq!(
        api.search(ContentKind::Mod, query, 20, Some(&target))
            .unwrap()
            .offset,
        20
    );
    assert_eq!(api.versions("example", Some(&target)).unwrap().len(), 1);
    let request = server
        .requests
        .lock()
        .unwrap()
        .iter()
        .find(|r| r.starts_with("/api/search"))
        .unwrap()
        .clone();
    let url = reqwest::Url::parse(&format!("{}{request}", server.base)).unwrap();
    let values: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(values["query"], query);
    let facets: Vec<Vec<String>> = serde_json::from_str(&values["facets"]).unwrap();
    assert!(facets.contains(&vec!["versions:test-1".into()]));
    assert!(facets.contains(&vec!["categories:fabric".into()]));
}

#[test]
fn publishing_failure_restores_files_and_keeps_the_previous_registry() {
    let root = temporary();
    let server = Server::new();
    let directory = root.path().join("instance");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("a.jar"), b"old mod").unwrap();
    fs::write(directory.join(REGISTRY), b"old registry").unwrap();
    let mut stage = files::Stage::new(root.path()).unwrap();
    let payload = stage.directory("payload").unwrap();
    let first = payload.join("a.jar");
    fs::write(&first, b"new mod").unwrap();
    let record = payload.join(REGISTRY);
    fs::write(&record, b"new registry").unwrap();
    let changes = BTreeMap::from([
        ("a.jar".into(), Some(first)),
        ("b.jar".into(), Some(payload.join("missing"))),
        (REGISTRY.into(), Some(record)),
    ]);
    assert!(
        stage
            .commit(&directory, changes, &server.downloader())
            .is_err()
    );
    assert_eq!(fs::read(directory.join("a.jar")).unwrap(), b"old mod");
    assert_eq!(fs::read(directory.join(REGISTRY)).unwrap(), b"old registry");
    assert!(!directory.join("b.jar").exists());
}

fn archive(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn vanilla(server: &Server) {
    let client = b"vanilla client";
    let index = serde_json::to_vec(&json!({"objects":{}})).unwrap();
    let metadata = json!({"id":"test-1","type":"release","mainClass":"test.Main","downloads":{"client":{"url":format!("{}/client.jar",server.base),"sha1":hashes(client)["sha1"],"size":client.len()}},"assetIndex":{"id":"test-assets","url":format!("{}/index.json",server.base),"sha1":hashes(&index)["sha1"],"size":index.len()},"libraries":[],"arguments":{"jvm":["-cp","${classpath}"],"game":[]},"javaVersion":{"majorVersion":17}});
    let bytes = serde_json::to_vec(&metadata).unwrap();
    server.put("/client.jar", client.to_vec());
    server.put("/index.json", index);
    server.put("/version.json", bytes.clone());
    server.json("/mc/game/version_manifest_v2.json",json!({"versions":[{"id":"test-1","type":"release","url":format!("{}/version.json",server.base),"sha1":hashes(&bytes)["sha1"]}]}));
    server.json("/fabric-meta/v2/versions/loader/test-1/0.20.1/profile/json",json!({"id":"fabric-test","inheritsFrom":"test-1","mainClass":"test.KnotClient","libraries":[]}));
}

#[test]
fn mrpack_installs_exact_loader_and_client_files_into_a_new_instance() {
    let root = temporary();
    let server = Server::new();
    vanilla(&server);
    let jar = b"pack mod";
    server.put("/pack.jar", jar.to_vec());
    let file = |path: &str, client: &str| json!({"path":path,"hashes":hashes(jar),"fileSize":jar.len(),"downloads":[format!("{}/pack.jar",server.base)],"env":{"client":client,"server":"required"}});
    let index = json!({"formatVersion":1,"game":"minecraft","versionId":"1.0","name":"Test pack","dependencies":{"minecraft":"test-1","fabric-loader":"0.20.1"},"files":[file("mods/client.jar","required"),file("mods/server.jar","unsupported"),file("mods/optional.jar","optional")]});
    let path = root.path().join("pack.mrpack");
    fs::write(
        &path,
        archive(&[
            ("modrinth.index.json", serde_json::to_vec(&index).unwrap()),
            ("overrides/config/test.txt", b"common".to_vec()),
            ("client-overrides/config/test.txt", b"client".to_vec()),
            ("server-overrides/config/ignored.txt", b"server".to_vec()),
        ]),
    )
    .unwrap();
    let plan = inspect_pack(root.path(), &path).unwrap();
    assert_eq!(plan.optional_files, 1);
    assert_eq!(plan.download_count(), 1);
    let instance = install_pack(
        root.path(),
        &plan,
        &crate::store::Settings::default(),
        &server.downloader(),
    )
    .unwrap();
    let directory = instance.directory(root.path()).unwrap();
    assert!(directory.join("mods/client.jar").is_file());
    assert!(!directory.join("mods/server.jar").exists());
    assert!(!directory.join("mods/optional.jar").exists());
    assert_eq!(
        fs::read(directory.join("config/test.txt")).unwrap(),
        b"client"
    );
    assert!(!directory.join("config/ignored.txt").exists());
    assert_eq!(instance.version_id, "test-1-fabric-0.20.1");
    assert_eq!(
        instances::list(root.path())
            .iter()
            .filter(|i| i.id.starts_with("pack-"))
            .count(),
        1
    );
    assert!(
        check_instance(root.path(), &instance, &server.downloader())
            .unwrap()
            .errors
            .is_empty()
    );
    let mut optional = plan.clone();
    optional.include_optional = true;
    let second = install_pack(
        root.path(),
        &optional,
        &crate::store::Settings::default(),
        &server.downloader(),
    )
    .unwrap();
    assert_ne!(second.id, instance.id);
    assert!(
        second
            .directory(root.path())
            .unwrap()
            .join("mods/optional.jar")
            .is_file()
    );
    assert!(!directory.join("mods/optional.jar").exists());
}

#[test]
fn pack_cancellation_or_bad_hash_does_not_publish_an_instance() {
    for failure in ["cancel", "hash"] {
        let root = temporary();
        let server = Server::new();
        let bytes = b"client mod";
        let index = json!({"formatVersion":1,"game":"minecraft","versionId":"1","name":"Failed pack","dependencies":{"minecraft":"test-1","fabric-loader":"0.20.1"},"files":[{"path":"mods/client.jar","hashes":hashes(bytes),"fileSize":bytes.len(),"downloads":[format!("{}/file.jar",server.base)]}]});
        let path = root.path().join("failed.mrpack");
        fs::write(
            &path,
            archive(&[("modrinth.index.json", serde_json::to_vec(&index).unwrap())]),
        )
        .unwrap();
        let plan = inspect_pack(root.path(), &path).unwrap();
        let downloader = server.downloader();
        if failure == "cancel" {
            downloader.cancel.store(true, Ordering::Relaxed);
        } else {
            server.put("/file.jar", b"bad".to_vec());
        }
        assert!(
            install_pack(
                root.path(),
                &plan,
                &crate::store::Settings::default(),
                &downloader
            )
            .is_err()
        );
        assert!(!root.path().join("instances").exists());
    }
}

#[test]
fn mrpack_rejects_unsafe_paths_unknown_loaders_and_bad_download_hosts() {
    for failure in ["path", "loader", "host", "metadata"] {
        let root = temporary();
        let mut index = json!({"formatVersion":1,"game":"minecraft","versionId":"1","name":"Test","dependencies":{"minecraft":"test-1"},"files":[]});
        if failure == "loader" {
            index["dependencies"]["quilt-loader"] = json!("1");
        }
        if failure == "host" {
            index["files"] = json!([{"path":"mods/test.jar","hashes":hashes(b"jar"),"fileSize":3,"downloads":["https://example.com/file.jar"]}]);
        }
        let mut entries = vec![("modrinth.index.json", serde_json::to_vec(&index).unwrap())];
        if failure == "path" {
            entries.push(("overrides/../escape", b"bad".to_vec()));
        }
        if failure == "metadata" {
            entries.push(("overrides/nscl-instance.json", b"bad".to_vec()));
        }
        let path = root.path().join("bad.mrpack");
        fs::write(&path, archive(&entries)).unwrap();
        assert!(inspect_pack(root.path(), &path).is_err(), "{failure}");
        assert!(!root.path().join("instances").exists());
    }
}
