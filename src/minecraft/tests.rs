use super::{download::*, install::*, launch::*, model::*, *};
use serde_json::json;
use sha1::{Digest, Sha1};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

fn test_dir() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha1::digest(bytes))
}

struct Server {
    base: String,
    files: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn new() -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr().to_ip().unwrap());
        let files = Arc::new(Mutex::new(BTreeMap::<String, Vec<u8>>::new()));
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (worker_files, worker_requests, worker_stop) =
            (files.clone(), requests.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                match server.recv_timeout(Duration::from_millis(100)) {
                    Ok(Some(request)) => {
                        worker_requests.fetch_add(1, Ordering::Relaxed);
                        let payload = worker_files.lock().unwrap().get(request.url()).cloned();
                        let response = match payload {
                            Some(bytes) => tiny_http::Response::from_data(bytes),
                            None => {
                                tiny_http::Response::from_data(Vec::new()).with_status_code(404)
                            }
                        };
                        let _ = request.respond(response);
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        });
        Self {
            base,
            files,
            requests,
            stop,
            thread: Some(worker),
        }
    }
    fn put(&self, path: &str, data: Vec<u8>) {
        self.files.lock().unwrap().insert(path.to_owned(), data);
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
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);

        if let Some(worker) = self.thread.take() {
            worker.join().unwrap();
        }
    }
}

fn archive(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(bytes).unwrap();
    writer.finish().unwrap().into_inner()
}

struct Fixture {
    server: Server,
    temporary: tempfile::TempDir,
    root: PathBuf,
    entry: VersionEntry,
    version: Version,
    index: AssetIndex,
}

impl Fixture {
    fn new() -> Self {
        let server = Server::new();
        let temporary = test_dir();
        let root = temporary.path().join("game with spaces");
        let client = b"client fixture".to_vec();
        let library = b"library fixture".to_vec();
        let asset = b"asset fixture".to_vec();
        let native = archive("test-native.dll", b"native fixture");
        let logging = b"log config fixture".to_vec();
        let asset_hash = hash(&asset);
        let index_json = json!({"virtual":true,"map_to_resources":true,"objects":{"minecraft/test.txt":{"hash":asset_hash,"size":asset.len()},"minecraft/duplicate.txt":{"hash":asset_hash,"size":asset.len()}}});
        let index_bytes = serde_json::to_vec(&index_json).unwrap();
        let platform = Platform::current();
        let native_classifier = format!("natives-{}", platform.os);
        let artifact = |url: &str, path: Option<&str>, data: &[u8]| json!({"url":url,"path":path,"sha1":hash(data),"size":data.len()});
        let version_json = json!({
            "id":"test-1","type":"release","mainClass":"test.GameMain","javaVersion":{"majorVersion":17},
            "downloads":{"client":artifact("https://piston-data.mojang.com/v1/objects/client/client.jar",None,&client)},
            "assetIndex":{"id":"test-assets","url":"https://piston-meta.mojang.com/v1/packages/index/test-assets.json","sha1":hash(&index_bytes),"size":index_bytes.len()},
            "libraries":[{"name":"test:lib:1","downloads":{"artifact":artifact("https://libraries.minecraft.net/test/lib.jar",Some("test/lib.jar"),&library)}},
            {"name":"test:native:1","natives":{platform.os.clone():native_classifier.clone()},"downloads":{"classifiers":{native_classifier:artifact("https://libraries.minecraft.net/test/native.jar",Some("test/native.jar"),&native)}},"extract":{"exclude":["META-INF/"]}}],
            "arguments":{"jvm":["-Djava.library.path=${natives_directory}","-cp","${classpath}",{"rules":[{"action":"allow","os":{"name":"windows"}}],"value":["-Dos.name=Windows 10"]}],"game":["--username","${auth_player_name}","--gameDir","${game_directory}","--assetsDir","${game_assets}","--uuid","${auth_uuid}",{"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"}]},
            "logging":{"client":{"argument":"-Dlog4j.configurationFile=${path}","file":{"id":"log.xml","url":"https://piston-data.mojang.com/v1/objects/log/log.xml","sha1":hash(&logging),"size":logging.len()}}}
        });
        let version_bytes = serde_json::to_vec(&version_json).unwrap();
        let entry = VersionEntry {
            id: "test-1".into(),
            kind: "release".into(),
            url: "https://piston-meta.mojang.com/v1/packages/version/test-1.json".into(),
            sha1: hash(&version_bytes),
            release_time: String::new(),
        };
        server.put(
            "/mc/game/version_manifest_v2.json",
            serde_json::to_vec(&json!({"versions":[entry.clone()]})).unwrap(),
        );
        server.put("/v1/packages/version/test-1.json", version_bytes);
        server.put("/v1/packages/index/test-assets.json", index_bytes);
        server.put("/v1/objects/client/client.jar", client);
        server.put("/maven/test/lib.jar", library);
        server.put("/maven/test/native.jar", native);
        server.put("/v1/objects/log/log.xml", logging);
        server.put(
            &format!("/assets/{}/{}", &asset_hash[..2], asset_hash),
            asset,
        );
        Self {
            server,
            temporary,
            root,
            entry,
            version: serde_json::from_value(version_json).unwrap(),
            index: serde_json::from_value(index_json).unwrap(),
        }
    }
}

#[test]
fn mirror_routing_and_official_fallback() {
    let source = Source {
        kind: SourceKind::Bmclapi,
        ..Default::default()
    };
    for (input, expected) in [
        (
            "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
            "/mc/game/version_manifest_v2.json",
        ),
        (
            "https://libraries.minecraft.net/com/test.jar",
            "/maven/com/test.jar",
        ),
        (
            "https://resources.download.minecraft.net/ab/abcdef",
            "/assets/ab/abcdef",
        ),
        (
            "https://piston-data.mojang.com/v1/objects/hash/client.jar",
            "/v1/objects/hash/client.jar",
        ),
    ] {
        let candidates = source.candidates(input).unwrap();
        assert_eq!(
            candidates[0],
            format!("https://bmclapi2.bangbang93.com{expected}")
        );
        assert_eq!(candidates[1], input);
    }
    let source = Source {
        kind: SourceKind::Custom,
        custom_base: "https://mirror.example/repository/".into(),
        fallback_to_official: false,
    };
    assert_eq!(
        source
            .candidates("https://libraries.minecraft.net/com/test.jar")
            .unwrap(),
        ["https://mirror.example/repository/maven/com/test.jar"]
    );
    assert_eq!(
        Source::default()
            .candidates("https://mirror.minguncle.cn/v1/packages/hash/test.json")
            .unwrap(),
        ["https://piston-meta.mojang.com/v1/packages/hash/test.json"]
    );
}

#[test]
fn invalid_mirrors_and_unsafe_paths_are_rejected() {
    for value in [
        "example.com",
        "file:///tmp",
        "https://user:secret@example.com",
        "https://example.com/?token=x",
    ] {
        assert!(
            Source {
                kind: SourceKind::Custom,
                custom_base: value.into(),
                ..Default::default()
            }
            .base()
            .is_err()
        );
    }
    for value in [
        "../evil",
        "/absolute",
        "C:/escape",
        "a\\..\\escape",
        "a/../escape",
        "a//b",
        "a/.. /escape",
        "NUL.dll",
        "a/b.",
        "evil?.dll",
        ".",
        "",
    ] {
        assert!(
            safe_join(std::path::Path::new("root"), value).is_err(),
            "{value}"
        );
    }
    for id in ["../evil", "C:", ".", "a/b"] {
        assert!(validate_id(id).is_err());
    }
}

#[test]
fn mirrored_catalog_urls_are_normalized_before_switching_sources() {
    let fixture = Fixture::new();
    let mut entry = fixture.entry.clone();
    entry.url = format!("{}/v1/packages/version/test-1.json", fixture.server.base);
    fixture.server.put(
        "/mc/game/version_manifest_v2.json",
        serde_json::to_vec(&json!({"versions":[entry]})).unwrap(),
    );
    let catalog = manifest(&fixture.server.downloader()).unwrap();
    assert_eq!(catalog.versions[0].url, fixture.entry.url);
}

#[test]
fn complete_install_reuses_cache_and_repairs_corruption() {
    let fixture = Fixture::new();
    let downloader = fixture.server.downloader();
    let versions = manifest(&downloader).unwrap();
    assert_eq!(versions.versions[0].id, "test-1");
    install(&fixture.root, &fixture.entry, &downloader).unwrap();
    assert_eq!(installed_versions(&fixture.root).len(), 1);
    assert_eq!(
        fs::read(
            fixture
                .root
                .join("assets/virtual/test-assets/minecraft/test.txt")
        )
        .unwrap(),
        b"asset fixture"
    );
    assert_eq!(
        fs::read(
            fixture
                .root
                .join("instances/test-1/resources/minecraft/duplicate.txt")
        )
        .unwrap(),
        b"asset fixture"
    );
    assert!(
        fixture
            .root
            .join("versions/test-1/natives")
            .join(Platform::current().key())
            .join("test-native.dll")
            .is_file()
    );
    let requests = fixture.server.requests.load(Ordering::Relaxed);
    install(&fixture.root, &fixture.entry, &downloader).unwrap();
    assert_eq!(fixture.server.requests.load(Ordering::Relaxed), requests);
    fs::write(fixture.root.join("libraries/test/lib.jar"), b"bad").unwrap();
    install(&fixture.root, &fixture.entry, &downloader).unwrap();
    assert_eq!(
        fixture.server.requests.load(Ordering::Relaxed),
        requests + 1
    );
    assert_eq!(
        fs::read(fixture.root.join("libraries/test/lib.jar")).unwrap(),
        b"library fixture"
    );
    assert!(fixture.temporary.path().is_dir());
}

#[test]
fn checksum_failure_never_marks_install_complete() {
    let fixture = Fixture::new();
    fixture
        .server
        .put("/v1/objects/client/client.jar", b"broken fixture".to_vec());
    let error = install(&fixture.root, &fixture.entry, &fixture.server.downloader()).unwrap_err();
    assert!(error.contains("SHA-1"), "{error}");
    assert!(installed_versions(&fixture.root).is_empty());
    assert!(
        !fixture
            .root
            .join("versions/test-1/test-1.jar.part")
            .exists()
    );
}

#[test]
fn failed_mirror_and_hash_mismatch_fall_back_to_the_next_source() {
    let server = Server::new();
    let directory = test_dir();
    let good = b"verified payload";
    server.put("/bad", b"tampered payload".to_vec());
    server.put("/good", good.to_vec());
    let job = Job {
        url: format!("{}/good", server.base),
        path: directory.path().join("client.jar"),
        sha1: hash(good),
        size: Some(good.len() as u64),
    };
    server
        .downloader()
        .download_from(
            &job,
            &[
                format!("{}/bad", server.base),
                format!("{}/good", server.base),
            ],
        )
        .unwrap();
    assert_eq!(fs::read(job.path).unwrap(), good);
}

#[test]
fn cancellation_does_not_publish_an_instance() {
    let fixture = Fixture::new();
    let downloader = fixture.server.downloader();
    downloader.cancel.store(true, Ordering::Relaxed);
    assert!(
        install(&fixture.root, &fixture.entry, &downloader)
            .unwrap_err()
            .contains("取消")
    );
    assert!(installed_versions(&fixture.root).is_empty());
}

#[test]
fn native_zip_cannot_escape_its_directory() {
    let fixture = Fixture::new();
    fixture
        .server
        .put("/maven/test/native.jar", archive("../escape.dll", b"evil"));
    let path = fixture.temporary.path().join("native.jar");
    fs::write(&path, archive("../escape.dll", b"evil")).unwrap();
    assert!(
        extract_natives(
            &fixture.root,
            "test-1",
            &Platform::current(),
            &[(path, Vec::new())],
            None
        )
        .is_err()
    );
    assert!(
        !fixture
            .root
            .join("versions/test-1/natives/escape.dll")
            .exists()
    );
}

#[test]
fn ordered_platform_and_feature_rules_follow_metadata() {
    let platform = Platform {
        os: "windows".into(),
        arch: "x86_64".into(),
        version: "10.0.26100".into(),
    };
    let rules:Vec<Rule>=serde_json::from_value(json!([{"action":"allow"},{"action":"disallow","os":{"name":"windows","arch":"x86_64","version":"^10\\."}},{"action":"allow","features":{"is_demo_user":true}}])).unwrap();
    assert!(!rules_allow(&rules, &platform, false));
    assert!(rules_allow(&rules, &platform, true));
    assert!(rules_allow(&[], &platform, false));
}

#[test]
fn modern_and_legacy_launch_arguments_preserve_spaces_and_demo_rules() {
    let fixture = Fixture::new();
    let root = &fixture.root;
    let platform = Platform::current();
    let settings = crate::store::Settings {
        player_name: "Notch".into(),
        ..Default::default()
    };
    let paths = vec![
        root.join("libraries/a jar.jar"),
        root.join("versions/test-1/test-1.jar"),
    ];
    let args = launch_arguments(
        root,
        &fixture.version,
        &fixture.index,
        &paths,
        &root.join("natives"),
        &platform,
        &settings,
    )
    .unwrap();
    assert!(
        args.iter()
            .any(|arg| std::path::Path::new(arg) == root.join("instances/test-1"))
    );
    assert!(args.iter().any(|arg| arg.contains("a jar.jar")));
    assert!(args.iter().any(|arg| arg == "-Xmx4096M"));
    assert!(!args.iter().any(|arg| arg == "--demo"));
    assert!(!args.iter().any(|arg| arg.contains("${")));
    assert_eq!(offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    let mut legacy = fixture.version.clone();
    legacy.arguments = None;
    legacy.legacy_arguments = Some(
        "--username ${auth_player_name} --gameDir ${game_directory} --assetsDir ${game_assets}"
            .into(),
    );
    let demo = crate::store::Settings {
        demo_mode: true,
        ..settings
    };
    let args = launch_arguments(
        root,
        &legacy,
        &fixture.index,
        &paths,
        &root.join("natives"),
        &platform,
        &demo,
    )
    .unwrap();
    assert!(args.iter().any(|arg| arg == "--demo"));
    assert!(args.iter().any(|arg| arg.ends_with("test-assets")));
}

#[test]
fn java_versions_and_player_names_are_validated() {
    assert_eq!(java_major("java version \"1.8.0_401\""), Some(8));
    assert_eq!(java_major("openjdk version \"17.0.10\""), Some(17));
    assert_eq!(java_major("java.version = 25.0.4"), Some(25));
    assert_eq!(java_major("invalid"), None);
    for name in ["", "Player Name", "中文", "01234567890123456"] {
        assert!(validate_player_name(name).is_err());
    }
    assert!(validate_player_name("NSCL_Player123").is_ok());
}

#[test]
fn online_arguments_use_msa_identity_and_the_selected_instance_directory() {
    let fixture = Fixture::new();
    let mut version = fixture.version.clone();
    version.arguments.as_mut().unwrap().game.extend(
        [
            "--accessToken",
            "${auth_access_token}",
            "--userType",
            "${user_type}",
            "--xuid",
            "${auth_xuid}",
            "--clientId",
            "${clientid}",
        ]
        .into_iter()
        .map(|s| Argument::Plain(s.into())),
    );
    let identity = LaunchIdentity {
        name: "OnlinePlayer".into(),
        uuid: "0123456789abcdef0123456789abcdef".into(),
        access_token: "PRIVATE-TEST-TOKEN".into(),
        xuid: "12345".into(),
        client_id: crate::accounts::DEFAULT_CLIENT_ID.into(),
        online: true,
    };
    let settings = crate::store::Settings {
        player_name: "OfflineName".into(),
        demo_mode: true,
        memory: 2048,
        ..Default::default()
    };
    let directory = fixture.root.join("instances/custom survival");
    let arguments = launch_arguments_for(
        &fixture.root,
        &version,
        &fixture.index,
        &[fixture.root.join("client.jar")],
        &fixture.root.join("natives"),
        &Platform::current(),
        LaunchContext {
            settings: &settings,
            identity: &identity,
            game_directory: &directory,
        },
    )
    .unwrap();
    for value in [
        "OnlinePlayer",
        "PRIVATE-TEST-TOKEN",
        "msa",
        "12345",
        crate::accounts::DEFAULT_CLIENT_ID,
        "-Xmx2048M",
    ] {
        assert!(arguments.iter().any(|a| a == value));
    }
    assert!(
        arguments
            .iter()
            .any(|a| std::path::Path::new(a) == directory)
    );
    assert!(
        !arguments
            .iter()
            .any(|a| a == "--demo" || a == "OfflineName")
    );
    let plan = LaunchPlan {
        java: "java".into(),
        arguments,
        game_directory: directory,
        log_path: "log".into(),
    };
    assert!(!format!("{plan:?}").contains("PRIVATE-TEST-TOKEN"));
}

#[test]
fn old_settings_migrate_without_losing_values() {
    let settings: crate::store::Settings =
        serde_json::from_str(r#"{"memory":2048,"java_path":"test-java"}"#).unwrap();
    assert_eq!(settings.memory, 2048);
    assert_eq!(settings.java_path, "test-java");
    assert_eq!(settings.source.kind, SourceKind::Official);
    assert_eq!(settings.player_name, "NSCLPlayer");
}

#[test]
fn damaged_install_is_rejected_before_executing_java() {
    let fixture = Fixture::new();
    install(&fixture.root, &fixture.entry, &fixture.server.downloader()).unwrap();
    fs::remove_file(fixture.root.join("versions/test-1/test-1.jar")).unwrap();
    let error =
        prepare_launch(&fixture.root, "test-1", &crate::store::Settings::default()).unwrap_err();
    assert!(error.contains("缺失或损坏"));
}

#[test]
#[ignore = "invoked by the process lifecycle test"]
fn child_process_fixture() {
    println!("test game stdout");
    eprintln!("test game stderr");
}

#[test]
fn game_process_preserves_working_directory_and_logs() {
    let directory = test_dir();
    let log_path = directory.path().join("game.log");
    let plan = LaunchPlan {
        java: std::env::current_exe().unwrap(),
        arguments: vec![
            "--exact".into(),
            "minecraft::tests::child_process_fixture".into(),
            "--ignored".into(),
            "--nocapture".into(),
        ],
        game_directory: directory.path().into(),
        log_path: log_path.clone(),
    };
    let mut started = false;
    assert_eq!(
        launch(&plan, |pid, path| {
            assert!(pid > 0);
            assert_eq!(path, &log_path);
            started = true;
        })
        .unwrap(),
        0
    );
    assert!(started);
    let log = fs::read_to_string(log_path).unwrap();
    assert!(log.contains("test game stdout"));
    assert!(log.contains("test game stderr"));
}
