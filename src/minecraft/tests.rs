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
                ..Default::default()
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
        (
            "https://meta.fabricmc.net/v2/versions/loader/1.20.1",
            "/fabric-meta/v2/versions/loader/1.20.1",
        ),
        (
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar",
            "/maven/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar",
        ),
        (
            "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml",
            "/maven/net/minecraftforge/forge/maven-metadata.xml",
        ),
        (
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
            "/maven/net/neoforged/neoforge/maven-metadata.xml",
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
        ..Default::default()
    };
    assert_eq!(
        source
            .candidates("https://libraries.minecraft.net/com/test.jar")
            .unwrap(),
        ["https://mirror.example/repository/maven/com/test.jar"]
    );
    assert_eq!(
        Source {
            kind: SourceKind::Official,
            ..Default::default()
        }
        .candidates("https://mirror.minguncle.cn/v1/packages/hash/test.json")
        .unwrap(),
        ["https://piston-meta.mojang.com/v1/packages/hash/test.json"]
    );
}

#[test]
fn automatic_source_tries_bangbang93_minguncle_then_official_and_preserves_manual_choices() {
    let source: Source = serde_json::from_value(json!({})).unwrap();
    assert_eq!(source.kind, SourceKind::Auto);
    assert_eq!(Source::default().kind, SourceKind::Auto);
    assert_eq!(
        Source {
            fallback_to_official: false,
            ..source
        }
        .candidates(source::MANIFEST_URL)
        .unwrap(),
        [
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json",
            "https://mirror.minguncle.cn/mc/game/version_manifest_v2.json",
            source::MANIFEST_URL
        ]
    );
    let manual: Source = serde_json::from_value(json!({"kind":"Official"})).unwrap();
    assert_eq!(
        manual.candidates(source::MANIFEST_URL).unwrap(),
        [source::MANIFEST_URL]
    );
    assert_eq!(
        Source::default()
            .candidates("https://api.modrinth.com/v2/search")
            .unwrap(),
        ["https://api.modrinth.com/v2/search"]
    );
}

#[test]
fn old_source_settings_keep_system_proxy_and_direct_mode_roundtrips() {
    let mut source: Source = serde_json::from_value(json!({
        "kind": "Official", "custom_base": "", "fallback_to_official": true
    }))
    .unwrap();
    assert_eq!(source.proxy_mode, ProxyMode::System);
    source.proxy_mode = ProxyMode::Direct;
    let restored: Source = serde_json::from_value(serde_json::to_value(&source).unwrap()).unwrap();
    assert_eq!(restored.proxy_mode, ProxyMode::Direct);
    assert_eq!(
        restored.candidates(source::MANIFEST_URL).unwrap(),
        [source::MANIFEST_URL]
    );
}

#[test]
#[ignore = "queries the public Mojang version list using both download connection modes"]
fn live_mojang_version_manifest() {
    let mut successes = 0;
    for proxy_mode in ProxyMode::ALL {
        let downloader = Downloader::new(
            Source {
                kind: SourceKind::Official,
                proxy_mode,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
            Arc::new(|_| {}),
        )
        .unwrap();
        match manifest(&downloader) {
            Ok(list) => {
                assert!(!list.versions.is_empty());
                println!("{}: {} versions", proxy_mode.label(), list.versions.len());
                successes += 1;
            }
            Err(error) => println!("{} failed: {error}", proxy_mode.label()),
        }
    }
    assert!(successes > 0, "both download connection modes failed");
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
    let mut settings: crate::store::Settings = serde_json::from_str(
        r#"{"memory":2048,"java_path":"test-java","demo_mode":true,"player_name":"LegacyPlayer"}"#,
    )
    .unwrap();
    assert_eq!(settings.memory, 2048);
    assert_eq!(settings.java_path, "test-java");
    assert_eq!(settings.source.kind, SourceKind::Auto);
    assert_eq!(settings.player_name, "LegacyPlayer");
    assert!(settings.auto_java && settings.auto_memory);
    assert_eq!(settings.configured_java(), "");
    assert!(!settings.demo_mode);
    settings.auto_java = false;
    settings.auto_memory = false;
    assert_eq!(settings.configured_java(), "test-java");
    let value = serde_json::to_value(&settings).unwrap();
    assert!(value.get("demo_mode").is_none());
    assert!(value.get("player_name").is_none());
    let restored: crate::store::Settings = serde_json::from_value(value).unwrap();
    assert!(!restored.auto_java && !restored.auto_memory);
    assert_eq!(restored.memory, 2048);
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
fn logging_reuses_verified_isolated_files_with_the_original_extension_for_java() {
    let fixture = Fixture::new();
    let downloader = fixture.server.downloader();
    let version_directory = fixture.root.join("versions/test-1");
    let game_directory = fixture.root.join("instances/isolated with spaces");
    let client = version_directory.join("test-1.jar");
    let identity = LaunchIdentity::offline("LogTest");
    let settings = crate::store::Settings::default();
    for path in [
        version_directory.join("downloads/log.json"),
        game_directory.join("assets/log_configs/log.xml"),
        game_directory.join("log_configs/log.xml"),
    ] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"log config fixture").unwrap();
        let resolved = super::logging::prepare(
            &fixture.root,
            &fixture.version,
            &client,
            &game_directory,
            &downloader,
        )
        .unwrap()
        .unwrap();
        let destination = fixture.root.join("assets/log_configs/log.xml");
        assert_eq!(resolved, destination);
        let arguments = launch_arguments_for(
            &fixture.root,
            &fixture.version,
            &fixture.index,
            std::slice::from_ref(&client),
            &version_directory.join("natives"),
            &Platform::current(),
            LaunchContext {
                settings: &settings,
                identity: &identity,
                game_directory: &game_directory,
            },
        )
        .unwrap();
        let argument = arguments
            .iter()
            .find_map(|argument| argument.strip_prefix("-Dlog4j.configurationFile="))
            .unwrap();
        assert_eq!(std::path::Path::new(argument), destination);
        assert_eq!(fs::read(&path).unwrap(), b"log config fixture");
        assert_eq!(fs::read(&destination).unwrap(), b"log config fixture");
        assert_eq!(fixture.server.requests.load(Ordering::Relaxed), 0);
        fs::remove_file(destination).unwrap();
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn missing_or_corrupt_logging_repairs_only_the_shared_configuration() {
    let fixture = Fixture::new();
    let downloader = fixture.server.downloader();
    let directory = fixture.root.join("versions/test-1");
    fs::create_dir_all(directory.join("downloads")).unwrap();
    let placeholder = directory.join("downloads/log.json");
    fs::write(&placeholder, b"").unwrap();
    let metadata = directory.join("test-1.json");
    let world = directory.join("saves/world/level.dat");
    fs::create_dir_all(world.parent().unwrap()).unwrap();
    fs::write(&metadata, b"original profile").unwrap();
    fs::write(&world, b"original world").unwrap();
    let destination = fixture.root.join("assets/log_configs/log.xml");
    for original in [None, Some(b"".as_slice()), Some(b"modified".as_slice())] {
        if let Some(bytes) = original {
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(&destination, bytes).unwrap();
        } else if destination.exists() {
            fs::remove_file(&destination).unwrap();
        }
        let requests = fixture.server.requests.load(Ordering::Relaxed);
        let resolved = super::logging::prepare(
            &fixture.root,
            &fixture.version,
            &directory.join("test-1.jar"),
            &directory,
            &downloader,
        )
        .unwrap();
        assert_eq!(resolved.as_ref(), Some(&destination));
        assert_eq!(fs::read(&destination).unwrap(), b"log config fixture");
        assert_eq!(
            fixture.server.requests.load(Ordering::Relaxed),
            requests + 1
        );
        super::logging::prepare(
            &fixture.root,
            &fixture.version,
            &directory.join("test-1.jar"),
            &directory,
            &downloader,
        )
        .unwrap();
        assert_eq!(
            fixture.server.requests.load(Ordering::Relaxed),
            requests + 1
        );
        assert!(fs::read(&placeholder).unwrap().is_empty());
        assert_eq!(fs::read(&metadata).unwrap(), b"original profile");
        assert_eq!(fs::read(&world).unwrap(), b"original world");
    }
}

#[test]
fn logging_download_failure_and_cancellation_preserve_original_files() {
    let fixture = Fixture::new();
    let downloader = fixture.server.downloader();
    let destination = fixture.root.join("assets/log_configs/log.xml");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(&destination, b"previous file").unwrap();
    fixture
        .server
        .put("/v1/objects/log/log.xml", b"wrong content".to_vec());
    let prepare = || {
        super::logging::prepare(
            &fixture.root,
            &fixture.version,
            &fixture.root.join("versions/test-1/test-1.jar"),
            &fixture.root.join("instances/test-1"),
            &downloader,
        )
    };
    let error = prepare().unwrap_err();
    assert!(error.contains("补全启动日志配置失败"), "{error}");
    assert!(error.contains("SHA-1"), "{error}");
    assert_eq!(fs::read(&destination).unwrap(), b"previous file");
    assert!(!destination.with_file_name("log.xml.part").exists());
    let requests = fixture.server.requests.load(Ordering::Relaxed);
    downloader.cancel.store(true, Ordering::Relaxed);
    assert_eq!(prepare().unwrap_err(), "操作已取消。");
    assert_eq!(fixture.server.requests.load(Ordering::Relaxed), requests);
}

#[test]
#[ignore = "requires a locally installed Java 17; validates complete launch preparation"]
fn real_java_launch_preparation_respects_isolation_without_duplicating_shared_assets() {
    use crate::instances::{self, Isolation};
    let fixture = Fixture::new();
    install(&fixture.root, &fixture.entry, &fixture.server.downloader()).unwrap();
    let java = crate::java::find("", 17).unwrap();
    let settings = crate::store::Settings {
        auto_java: false,
        java_path: java.to_string_lossy().into_owned(),
        auto_memory: false,
        source: fixture.server.downloader().source,
        ..Default::default()
    };
    let root = absolute_root(&fixture.root).unwrap();
    let identity = LaunchIdentity::offline("IsolationTest");
    let mut instance = instances::create(&fixture.root, "test-1", "Launch isolation").unwrap();
    let mut classpaths = Vec::new();
    for mode in Isolation::ALL {
        instance.isolation = mode;
        instances::save(&fixture.root, &instance).unwrap();
        let plan = prepare_instance_launch(&fixture.root, &instance, &settings, &identity).unwrap();
        let expected = if mode == Isolation::Shared {
            root.clone()
        } else {
            instance.directory(&root).unwrap()
        };
        assert_eq!(plan.game_directory, expected);
        assert_eq!(plan.log_path, expected.join("logs/nscl-launch.log"));
        let game_dir = plan
            .arguments
            .windows(2)
            .find(|a| a[0] == "--gameDir")
            .unwrap();
        assert_eq!(std::path::Path::new(&game_dir[1]), expected);
        classpaths.push(plan.arguments.windows(2).find(|a| a[0] == "-cp").unwrap()[1].clone());
    }
    assert_eq!(classpaths[0], classpaths[1]);
    let imported_id = "已有版本 [测试 Fabric]";
    let imported_dir = root.join("versions").join(imported_id);
    fs::create_dir_all(imported_dir.join("saves/world")).unwrap();
    fs::write(imported_dir.join("saves/world/level.dat"), b"keep world").unwrap();
    fs::copy(
        root.join("versions/test-1/test-1.jar"),
        imported_dir.join(format!("{imported_id}.jar")),
    )
    .unwrap();
    let mut metadata = serde_json::to_value(&fixture.version).unwrap();
    metadata["id"] = imported_id.into();
    crate::store::write_json(&imported_dir.join(format!("{imported_id}.json")), &metadata).unwrap();
    let original = fs::read(imported_dir.join(format!("{imported_id}.json"))).unwrap();
    let imported = instances::list(&root)
        .into_iter()
        .find(|i| i.version_id == imported_id)
        .unwrap();
    assert_eq!(imported.isolation, Isolation::Version);
    let shared_logging = root.join("assets/log_configs/log.xml");
    let isolated_logging = imported_dir.join("downloads/log.json");
    fs::create_dir_all(isolated_logging.parent().unwrap()).unwrap();
    fs::rename(&shared_logging, &isolated_logging).unwrap();
    let requests = fixture.server.requests.load(Ordering::Relaxed);
    let plan = prepare_instance_launch(&root, &imported, &settings, &identity).unwrap();
    assert_eq!(plan.game_directory, imported_dir);
    let logging_argument = |plan: &LaunchPlan| {
        PathBuf::from(
            plan.arguments
                .iter()
                .find_map(|argument| argument.strip_prefix("-Dlog4j.configurationFile="))
                .unwrap(),
        )
    };
    assert_eq!(logging_argument(&plan), shared_logging);
    assert_eq!(fixture.server.requests.load(Ordering::Relaxed), requests);
    assert_eq!(fs::read(&shared_logging).unwrap(), b"log config fixture");
    fs::remove_file(&shared_logging).unwrap();
    fs::write(&isolated_logging, b"").unwrap();
    let plan = prepare_instance_launch(&root, &imported, &settings, &identity).unwrap();
    assert_eq!(logging_argument(&plan), shared_logging);
    assert_eq!(
        fixture.server.requests.load(Ordering::Relaxed),
        requests + 1
    );
    assert!(fs::read(&isolated_logging).unwrap().is_empty());
    assert_eq!(
        fs::read(imported_dir.join(format!("{imported_id}.json"))).unwrap(),
        original
    );
    assert_eq!(
        fs::read(imported_dir.join("saves/world/level.dat")).unwrap(),
        b"keep world"
    );
}

#[test]
#[ignore = "invoked by the process lifecycle test"]
fn child_process_fixture() {
    println!("test game stdout");
    eprintln!("test game stderr");
    if let Ok(failure) = fs::read_to_string("fixture-failure.txt") {
        eprintln!("{failure}");
        std::process::exit(1);
    }
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

#[test]
fn fabric_remap_failure_is_diagnosed_without_hiding_other_exit_codes() {
    let fabric_error = "java.lang.RuntimeException: error remapping game jars\nCaused by: java.io.IOException: client-intermediary.jar.tmp can't be written\n at net.fabricmc.loader.impl.lib.tinyremapper.OutputConsumerPath.<init>";
    for (output, fabric) in [(fabric_error, true), ("Unrelated game crash", false)] {
        let directory = test_dir();
        fs::write(directory.path().join("fixture-failure.txt"), output).unwrap();
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
        let result = launch(&plan, |_, _| started = true);
        assert!(started);
        assert!(fs::read_to_string(&log_path).unwrap().contains(output));
        if fabric {
            let error = result.unwrap_err();
            assert!(error.contains("Fabric 首次映射失败"));
            assert!(error.contains("退出码：1"));
            assert!(error.contains(log_path.to_string_lossy().as_ref()));
            assert!(error.contains("目录"));
        } else {
            assert_eq!(result.unwrap(), 1);
        }
    }
}

#[test]
fn fabric_installs_merges_libraries_and_keeps_vanilla_and_worlds() {
    let fixture = Fixture::new();
    let bytes = b"new fabric library";
    let selection = LoaderVersion {
        kind: LoaderKind::Fabric,
        minecraft: fixture.entry.id.clone(),
        version: "0.20.1".into(),
        stable: true,
    };
    let profile = json!({
        "id":"fabric-native-id", "inheritsFrom":fixture.entry.id,
        "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
        "arguments":{"game":["--loader-test","value with spaces"],"jvm":["-Dfabric.test=true"]},
        "libraries":[{"name":"test:lib:2","url":"https://maven.fabricmc.net/","sha1":hash(bytes),"size":bytes.len()}]
    });
    fixture.server.put(
        "/fabric-meta/v2/versions/loader/test-1/0.20.1/profile/json",
        serde_json::to_vec(&profile).unwrap(),
    );
    fixture.server.put(
        "/fabric-meta/v2/versions/loader/test-1",
        serde_json::to_vec(&json!([{"loader":{"version":"0.20.1","stable":true}}])).unwrap(),
    );
    fixture
        .server
        .put("/maven/test/lib/2/lib-2.jar", bytes.to_vec());
    let downloader = fixture.server.downloader();
    assert_eq!(
        loader_versions("test-1", LoaderKind::Fabric, &downloader).unwrap()[0],
        selection
    );
    let installed =
        install_loader(&fixture.root, &fixture.entry, &selection, "", &downloader).unwrap();
    assert_eq!(installed.loader.as_ref(), Some(&selection));
    assert_eq!(installed_versions(&fixture.root).len(), 2);
    let metadata = fixture
        .root
        .join("versions")
        .join(&installed.id)
        .join(format!("{}.json", installed.id));
    let version: Version = read_json(&metadata).unwrap();
    assert!(version.inherits_from.is_none());
    assert!(
        version
            .libraries
            .iter()
            .any(|library| library.name == "test:lib:2")
    );
    assert!(
        !version
            .libraries
            .iter()
            .any(|library| library.name == "test:lib:1")
    );
    assert!(fixture.root.join("libraries/test/lib.jar").is_file());
    let platform = Platform::current();
    let files = plan(&fixture.root, &version, &fixture.index, &platform).unwrap();
    let args = launch_arguments(
        &fixture.root,
        &version,
        &fixture.index,
        &files.classpath,
        &fixture.root.join("natives"),
        &platform,
        &crate::store::Settings::default(),
    )
    .unwrap();
    for expected in [
        "net.fabricmc.loader.impl.launch.knot.KnotClient",
        "--loader-test",
        "value with spaces",
        "--username",
        "-Dfabric.test=true",
    ] {
        assert!(args.iter().any(|arg| arg == expected));
    }
    let instance = crate::instances::list(&fixture.root)
        .into_iter()
        .find(|instance| instance.version_id == installed.id)
        .unwrap();
    let world = instance
        .directory(&fixture.root)
        .unwrap()
        .join("saves/test-world");
    fs::create_dir_all(&world).unwrap();
    fs::write(world.join("level.dat"), b"keep my world").unwrap();
    install_loader(&fixture.root, &fixture.entry, &selection, "", &downloader).unwrap();
    assert_eq!(fs::read(world.join("level.dat")).unwrap(), b"keep my world");
    let mut wrong_parent = profile;
    wrong_parent["inheritsFrom"] = "other-game".into();
    fixture.server.put(
        "/fabric-meta/v2/versions/loader/test-1/0.20.1/profile/json",
        serde_json::to_vec(&wrong_parent).unwrap(),
    );
    assert!(install_loader(&fixture.root, &fixture.entry, &selection, "", &downloader).is_err());
    assert!(!metadata.with_file_name("nscl-instance.json").exists());
    assert_eq!(fs::read(world.join("level.dat")).unwrap(), b"keep my world");
}

fn loader_installer(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn forge_and_neoforge_install_embedded_libraries_and_check_them_before_launch() {
    for (kind, version, path) in [
        (
            LoaderKind::Forge,
            "47.4.28",
            "/maven/net/minecraftforge/forge/test-1-47.4.28/forge-test-1-47.4.28-installer.jar",
        ),
        (
            LoaderKind::NeoForge,
            "21.1.264",
            "/maven/net/neoforged/neoforge/21.1.264/neoforge-21.1.264-installer.jar",
        ),
    ] {
        let fixture = Fixture::new();
        let embedded = b"embedded universal jar";
        let relative = "net/test/universal/1/universal-1.jar";
        let profile = json!({"minecraft":fixture.entry.id,"json":"/version.json","processors":[{"sides":["server"],"jar":"bad:server:1","args":["--never-run"]}]});
        let child = json!({"id":"native-profile","inheritsFrom":fixture.entry.id,"mainClass":"net.test.LoaderMain","arguments":{"game":["--launchTarget","testclient"]},"libraries":[{"name":"net.test:universal:1","downloads":{"artifact":{"url":"","sha1":hash(embedded),"path":relative,"size":embedded.len()}}}]});
        let installer = loader_installer(&[
            (
                "install_profile.json",
                serde_json::to_vec(&profile).unwrap(),
            ),
            ("version.json", serde_json::to_vec(&child).unwrap()),
            (&format!("maven/{relative}"), embedded.to_vec()),
        ]);
        fixture.server.put(path, installer.clone());
        fixture
            .server
            .put(&format!("{path}.sha1"), hash(&installer).into_bytes());
        let selection = LoaderVersion {
            kind,
            minecraft: fixture.entry.id.clone(),
            version: version.into(),
            stable: true,
        };
        // Server-only processors must not require Java or execute their payload.
        let installed = install_loader(
            &fixture.root,
            &fixture.entry,
            &selection,
            "missing-java.exe",
            &fixture.server.downloader(),
        )
        .unwrap();
        assert_eq!(installed.extra_files.len(), 1);
        assert_eq!(
            fs::read(fixture.root.join("libraries").join(relative)).unwrap(),
            embedded
        );
        let marker = fixture
            .root
            .join("versions")
            .join(&installed.id)
            .join("nscl-instance.json");
        assert!(marker.is_file());
        fs::write(fixture.root.join("libraries").join(relative), b"damaged").unwrap();
        let error = prepare_launch(
            &fixture.root,
            &installed.id,
            &crate::store::Settings::default(),
        )
        .unwrap_err();
        assert!(error.contains("加载器生成文件缺失或损坏"));
        install_loader(
            &fixture.root,
            &fixture.entry,
            &selection,
            "missing-java.exe",
            &fixture.server.downloader(),
        )
        .unwrap();
        assert_eq!(
            fs::read(fixture.root.join("libraries").join(relative)).unwrap(),
            embedded
        );
    }
}

#[test]
fn forge_client_ignores_unused_server_data_but_validates_active_processor_outputs() {
    for corrupted in [false, true] {
        let fixture = Fixture::new();
        let patched = b"verified client patch";
        let relative = "net/test/patched/1/patched-1.jar";
        let profile = json!({
            "minecraft":fixture.entry.id, "json":"/version.json",
            "data":{
                "MC_UNPACKED":{"client":"[net.minecraft:client:test-1]","server":"[net.minecraft:server:test-1:unpacked]"},
                "MC_UNPACKED_SHA":{"client":format!("'{}'",fixture.version.downloads["client"].sha1),"server":format!("'{}'",hash(b"server jar"))},
                "PATCHED":{"client":"[net.test:patched:1]","server":"[net.test:server-patched:1]"},
                "PATCHED_SHA":{"client":format!("'{}'",hash(patched)),"server":format!("'{}'",hash(b"server patch"))}
            },
            "processors":[
                {"sides":["server"],"jar":"missing:server:1","args":["{MC_UNPACKED}"],"outputs":{"{MC_UNPACKED}":"{MC_UNPACKED_SHA}"}},
                {"sides":["client"],"jar":"missing:client:1","args":["--clean","{MINECRAFT_JAR}","--output","{PATCHED}"],"outputs":{"{PATCHED}":"{PATCHED_SHA}"}}
            ]
        });
        let child = json!({
            "id":"native-profile", "inheritsFrom":fixture.entry.id, "mainClass":"net.test.LoaderMain",
            "libraries":[{"name":"net.test:patched:1","downloads":{"artifact":{"url":"","sha1":hash(patched),"path":relative,"size":patched.len()}}}]
        });
        let processor = loader_installer(&[(
            "META-INF/MANIFEST.MF",
            b"Manifest-Version: 1.0\r\nMain-Class: test.ClientProcessor\r\n\r\n".to_vec(),
        )]);
        let installer = loader_installer(&[
            (
                "install_profile.json",
                serde_json::to_vec(&profile).unwrap(),
            ),
            ("version.json", serde_json::to_vec(&child).unwrap()),
            ("maven/missing/client/1/client-1.jar", processor),
            (
                &format!("maven/{relative}"),
                if corrupted {
                    b"damaged client patch".to_vec()
                } else {
                    patched.to_vec()
                },
            ),
        ]);
        let path =
            "/maven/net/minecraftforge/forge/test-1-47.4.28/forge-test-1-47.4.28-installer.jar";
        fixture.server.put(path, installer.clone());
        fixture
            .server
            .put(&format!("{path}.sha1"), hash(&installer).into_bytes());
        let selection = LoaderVersion {
            kind: LoaderKind::Forge,
            minecraft: fixture.entry.id.clone(),
            version: "47.4.28".into(),
            stable: true,
        };
        let result = install_loader(
            &fixture.root,
            &fixture.entry,
            &selection,
            "missing-java.exe",
            &fixture.server.downloader(),
        );
        let marker = fixture
            .root
            .join("versions")
            .join(selection.id().unwrap())
            .join("nscl-instance.json");
        if corrupted {
            // A bad declared output must require rebuilding; it cannot publish a profile
            // when the rebuild's Java runtime is unavailable.
            let error = result.unwrap_err();
            assert!(error.contains("Java 路径不存在"), "{error}");
            assert!(!marker.exists());
        } else {
            let installed = result.unwrap();
            assert!(marker.is_file());
            assert!(
                installed
                    .extra_files
                    .iter()
                    .any(|file| file.path.as_deref() == Some(relative))
            );
            assert!(
                !fixture
                    .root
                    .join("libraries/net/minecraft/client/test-1/client-test-1.jar")
                    .exists()
            );
        }
    }
}

#[test]
fn bad_installer_checksum_zip_paths_and_cancellation_never_publish_loader() {
    for failure in ["checksum", "zip-slip", "cancel"] {
        let fixture = Fixture::new();
        let profile = json!({"minecraft":fixture.entry.id,"json":"/version.json"});
        let child = json!({"id":"native-profile","inheritsFrom":fixture.entry.id,"mainClass":"net.test.LoaderMain","libraries":[]});
        let installer = loader_installer(&[
            (
                "install_profile.json",
                serde_json::to_vec(&profile).unwrap(),
            ),
            ("version.json", serde_json::to_vec(&child).unwrap()),
            ("maven/../escaped.jar", b"must never escape".to_vec()),
        ]);
        let path =
            "/maven/net/minecraftforge/forge/test-1-47.4.28/forge-test-1-47.4.28-installer.jar";
        fixture.server.put(path, installer.clone());
        fixture.server.put(
            &format!("{path}.sha1"),
            if failure == "checksum" {
                hash(b"wrong installer").into_bytes()
            } else {
                hash(&installer).into_bytes()
            },
        );
        let selection = LoaderVersion {
            kind: LoaderKind::Forge,
            minecraft: fixture.entry.id.clone(),
            version: "47.4.28".into(),
            stable: true,
        };
        let mut downloader = fixture.server.downloader();
        if failure == "cancel" {
            downloader.cancel = Arc::new(AtomicBool::new(true));
        }
        let error =
            install_loader(&fixture.root, &fixture.entry, &selection, "", &downloader).unwrap_err();
        assert!(!error.is_empty());
        assert!(
            !installed_versions(&fixture.root)
                .iter()
                .any(|item| item.loader.is_some())
        );
        assert!(!fixture.root.join("escaped.jar").exists());
    }
}
