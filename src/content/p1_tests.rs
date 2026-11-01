use super::*;
use crate::content::{curseforge::CurseForge, pack};
use crate::instances::backup;

fn pack_file(root: &Path, version: &str, overrides: &[(&str, Vec<u8>)]) -> PackPlan {
    pack_file_for(root, version, "test-1", overrides)
}
fn pack_file_for(
    root: &Path,
    version: &str,
    game: &str,
    overrides: &[(&str, Vec<u8>)],
) -> PackPlan {
    let index = json!({"formatVersion":1,"game":"minecraft","versionId":version,"name":"Upgrade test","dependencies":{"minecraft":game,"fabric-loader":"0.20.1"},"files":[]});
    let mut entries = vec![("modrinth.index.json", serde_json::to_vec(&index).unwrap())];
    entries.extend_from_slice(overrides);
    let path = root.join(format!("pack-{version}.mrpack"));
    fs::write(&path, archive(&entries)).unwrap();
    inspect_pack(root, &path).unwrap()
}
fn jar(id: &str, version: &str, depends: serde_json::Value) -> Vec<u8> {
    archive(&[(
        "fabric.mod.json",
        serde_json::to_vec(
            &json!({"schemaVersion":1,"id":id,"name":id,"version":version,"depends":depends}),
        )
        .unwrap(),
    )])
}

#[test]
fn pack_upgrade_keeps_worlds_edits_user_mods_disabled_state_and_custom_roots_then_rolls_back() {
    let root = temporary();
    let server = Server::new();
    vanilla(&server);
    let d = server.downloader();
    let settings = crate::store::Settings {
        backup_directory: root.path().join("snapshots").to_string_lossy().into_owned(),
        ..Default::default()
    };
    let first = pack_file(
        root.path(),
        "v1",
        &[
            (
                "overrides/mods/bundled-1.jar",
                jar("bundled", "1.0.0", json!({})),
            ),
            ("overrides/config/edited.txt", b"old default".to_vec()),
            ("overrides/custom-root/owned.txt", b"old custom".to_vec()),
            ("overrides/options.txt", b"old options".to_vec()),
        ],
    );
    let instance = install_pack(root.path(), &first, &settings, &d).unwrap();
    let directory = instance.game_directory(root.path()).unwrap();
    fs::create_dir_all(directory.join("saves/world")).unwrap();
    fs::write(directory.join("saves/world/level.dat"), b"my world").unwrap();
    fs::write(
        directory.join("mods/user.jar"),
        jar("user", "1.0.0", json!({})),
    )
    .unwrap();
    fs::write(directory.join("config/edited.txt"), b"my edits").unwrap();
    fs::write(directory.join("options.txt"), b"my options").unwrap();
    let local = list_local(root.path(), &instance, LocalKind::Mods, &d).unwrap();
    let bundled = local
        .entries
        .iter()
        .find(|e| e.filename == "bundled-1.jar")
        .unwrap();
    change_local(root.path(), &local, bundled, LocalAction::Disable, &d).unwrap();
    let next = pack_file(
        root.path(),
        "v2",
        &[
            (
                "overrides/mods/bundled-2.jar",
                jar("bundled", "2.0.0", json!({})),
            ),
            ("overrides/config/edited.txt", b"new default".to_vec()),
            ("overrides/custom-root/owned.txt", b"new custom".to_vec()),
            ("overrides/new-root/new.txt", b"new root".to_vec()),
            ("overrides/options.txt", b"new options".to_vec()),
        ],
    );
    let plan = plan_update(root.path(), &instance, next, &d).unwrap();
    assert!(plan.conflicts.contains(&"config/edited.txt".into()));
    let updated = install_update(root.path(), &plan, &settings, &d).unwrap();
    assert_eq!(updated.id, instance.id);
    assert_eq!(registry(&directory).unwrap().pack.unwrap().version, "v2");
    assert_eq!(
        fs::read(directory.join("config/edited.txt")).unwrap(),
        b"my edits"
    );
    assert_eq!(
        fs::read(directory.join("options.txt")).unwrap(),
        b"my options"
    );
    assert_eq!(
        fs::read(directory.join("saves/world/level.dat")).unwrap(),
        b"my world"
    );
    assert!(directory.join("mods/user.jar").exists());
    assert!(!directory.join("mods/bundled-2.jar").exists());
    assert!(directory.join("mods/bundled-2.jar.disabled").exists());
    assert!(!directory.join("mods/bundled-1.jar.disabled").exists());
    assert_eq!(
        fs::read(directory.join("custom-root/owned.txt")).unwrap(),
        b"new custom"
    );
    let backups = backup::list(&backup::directory(&settings), &updated).unwrap();
    assert_eq!(backups.len(), 1);
    backup::restore(
        root.path(),
        &updated,
        &backup::directory(&settings),
        &backups[0].id,
        &d,
    )
    .unwrap();
    assert_eq!(registry(&directory).unwrap().pack.unwrap().version, "v1");
    assert_eq!(
        fs::read(directory.join("custom-root/owned.txt")).unwrap(),
        b"old custom"
    );
    assert!(!directory.join("new-root").exists());
    assert!(directory.join("mods/bundled-1.jar.disabled").exists());
    assert!(!directory.join("mods/bundled-2.jar.disabled").exists());
    assert!(directory.join("mods/user.jar").exists());
    assert_eq!(
        backup::list(&backup::directory(&settings), &updated)
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn stale_or_cancelled_pack_upgrades_leave_the_original_instance_intact() {
    let root = temporary();
    let server = Server::new();
    vanilla(&server);
    let d = server.downloader();
    let settings = crate::store::Settings {
        backup_directory: root.path().join("snapshots").to_string_lossy().into_owned(),
        ..Default::default()
    };
    let first = pack_file(
        root.path(),
        "v1",
        &[("overrides/config/a.txt", b"v1".to_vec())],
    );
    let instance = install_pack(root.path(), &first, &settings, &d).unwrap();
    let directory = instance.game_directory(root.path()).unwrap();
    let next = pack_file(
        root.path(),
        "v2",
        &[("overrides/config/a.txt", b"v2".to_vec())],
    );
    let plan = plan_update(root.path(), &instance, next.clone(), &d).unwrap();
    fs::write(directory.join("config/a.txt"), b"changed after preview").unwrap();
    assert!(
        install_update(root.path(), &plan, &settings, &d)
            .unwrap_err()
            .contains("预览后")
    );
    let plan = plan_update(root.path(), &instance, next, &d).unwrap();
    d.cancel.store(true, Ordering::Relaxed);
    assert!(install_update(root.path(), &plan, &settings, &d).is_err());
    assert_eq!(
        fs::read(directory.join("config/a.txt")).unwrap(),
        b"changed after preview"
    );
    assert_eq!(registry(&directory).unwrap().pack.unwrap().version, "v1");
}

#[test]
fn pack_upgrade_can_change_minecraft_and_restore_the_previous_runtime_configuration() {
    let root = temporary();
    let server = Server::new();
    vanilla(&server);
    let d = server.downloader();
    let settings = crate::store::Settings {
        backup_directory: root.path().join("snapshots").to_string_lossy().into_owned(),
        ..Default::default()
    };
    let first = pack_file(root.path(), "first", &[]);
    let instance = install_pack(root.path(), &first, &settings, &d).unwrap();
    let bytes = server.routes.lock().unwrap()["/version.json"].clone();
    let mut metadata: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    metadata["id"] = json!("test-2");
    let next_bytes = serde_json::to_vec(&metadata).unwrap();
    server.put("/version-2.json", next_bytes.clone());
    server.json("/mc/game/version_manifest_v2.json", json!({"versions":[
        {"id":"test-1","type":"release","url":format!("{}/version.json", server.base),"sha1":hashes(&bytes)["sha1"]},
        {"id":"test-2","type":"release","url":format!("{}/version-2.json", server.base),"sha1":hashes(&next_bytes)["sha1"]}
    ]}));
    server.json("/fabric-meta/v2/versions/loader/test-2/0.20.1/profile/json", json!({"id":"fabric-test-2","inheritsFrom":"test-2","mainClass":"test.KnotClient","libraries":[]}));
    let next = pack_file_for(root.path(), "next", "test-2", &[]);
    let plan = plan_update(root.path(), &instance, next, &d).unwrap();
    let updated = install_update(root.path(), &plan, &settings, &d).unwrap();
    assert_eq!(updated.version_id, "test-2-fabric-0.20.1");
    let snapshots = backup::list(&backup::directory(&settings), &updated).unwrap();
    backup::restore(
        root.path(),
        &updated,
        &backup::directory(&settings),
        &snapshots[0].id,
        &d,
    )
    .unwrap();
    let restored = instances::list(root.path())
        .into_iter()
        .find(|i| i.id == instance.id)
        .unwrap();
    assert_eq!(restored.version_id, "test-1-fabric-0.20.1");
    assert_eq!(restored.launch, instance.launch);
    assert!(
        crate::minecraft::installed_versions(root.path())
            .iter()
            .any(|v| v.id == updated.version_id)
    );
}

#[test]
fn online_resources_install_update_keep_disabled_state_and_protect_user_files() {
    for kind in [ContentKind::ResourcePack, ContentKind::Shader] {
        let root = temporary();
        let server = Server::new();
        let api = server.api();
        let target = target(root.path());
        let directory = target.instance.game_directory(root.path()).unwrap();
        let make = |id: &str| {
            let bytes = archive(&[
                (
                    if kind == ContentKind::Shader {
                        "shaders/test.vsh"
                    } else {
                        "pack.mcmeta"
                    },
                    b"{}".to_vec(),
                ),
                ("version.txt", id.as_bytes().to_vec()),
            ]);
            server.put(&format!("/files/{id}.zip"), bytes.clone());
            server.json(
                "/api/project/resource",
                json!({"project_type":kind.key(),"client_side":"required"}),
            );
            let mut v = version(&server, "resource", id, json!([]));
            v.loaders = vec![];
            v.files[0].filename = format!("{id}.zip");
            v.files[0].url = format!("{}/files/{id}.zip", server.base);
            v.files[0].size = bytes.len() as u64;
            v.files[0].hashes = hashes(&bytes);
            server.json(
                "/api/project/resource",
                json!({"project_type":kind.key(),"client_side":"required"}),
            );
            v
        };
        let v1 = make("r1");
        let plan = plan_resource(root.path(), target.clone(), v1, kind, &api).unwrap();
        install_resource(root.path(), &plan, &api.downloader).unwrap();
        let local_kind = if kind == ContentKind::Shader {
            LocalKind::Shaders
        } else {
            LocalKind::ResourcePacks
        };
        let local = list_local(root.path(), &target.instance, local_kind, &api.downloader).unwrap();
        change_local(
            root.path(),
            &local,
            &local.entries[0],
            LocalAction::Disable,
            &api.downloader,
        )
        .unwrap();
        let v2 = make("r2");
        let plan = plan_resource(root.path(), target.clone(), v2, kind, &api).unwrap();
        install_resource(root.path(), &plan, &api.downloader).unwrap();
        let path = directory.join(format!(".nscl/disabled/{}/r2.zip", local_kind.directory()));
        assert!(path.is_file());
        assert!(
            !directory
                .join(format!(".nscl/disabled/{}/r1.zip", local_kind.directory()))
                .exists()
        );
        fs::write(&path, b"user changed").unwrap();
        let v3 = make("r3");
        let plan = plan_resource(root.path(), target.clone(), v3, kind, &api).unwrap();
        assert!(install_resource(root.path(), &plan, &api.downloader).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"user changed");
        fs::remove_file(&path).unwrap();
        install_resource(root.path(), &plan, &api.downloader).unwrap();
    }
}

#[test]
fn batch_updates_share_dependencies_and_respect_version_locks() {
    let root = temporary();
    let server = Server::new();
    let api = server.api();
    let target = target(root.path());
    let dep = version(&server, "shared", "dep", json!([]));
    let dependency =
        json!([{"project_id":"shared","version_id":dep.id,"dependency_type":"required"}]);
    let a = version(&server, "a", "a1", dependency.clone());
    let b = version(&server, "b", "b1", dependency.clone());
    let plan = plan_mods(root.path(), target.clone(), vec![a, b], &api).unwrap();
    assert_eq!(plan.versions.len(), 3);
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    lock_mod_versions(
        root.path(),
        &target,
        &["a".into()].into_iter().collect(),
        true,
    )
    .unwrap();
    let a2 = version(&server, "a", "a2", dependency.clone());
    let b2 = version(&server, "b", "b2", dependency);
    assert!(
        plan_mods(
            root.path(),
            target.clone(),
            vec![a2.clone(), b2.clone()],
            &api
        )
        .err()
        .unwrap()
        .contains("锁定")
    );
    lock_mod_versions(
        root.path(),
        &target,
        &["a".into()].into_iter().collect(),
        false,
    )
    .unwrap();
    let plan = plan_mods(root.path(), target, vec![a2, b2], &api).unwrap();
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    let directory = plan.target.instance.game_directory(root.path()).unwrap();
    assert_eq!(registry(&directory).unwrap().mods["a"].version.id, "a2");
    assert_eq!(registry(&directory).unwrap().mods["b"].version.id, "b2");
    assert!(!directory.join("mods/a-a1.jar").exists());
}

#[test]
fn runtime_metadata_checks_local_mod_dependencies_without_online_registration() {
    let root = temporary();
    let server = Server::new();
    let target = target(root.path());
    let directory = target
        .instance
        .game_directory(root.path())
        .unwrap()
        .join("mods");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("a.jar"),
        jar("a", "1.0.0", json!({"missing":">=2.0.0"})),
    )
    .unwrap();
    let report = check_instance(root.path(), &target.instance, &server.downloader()).unwrap();
    assert!(report.errors.iter().any(|s| s.contains("missing")));
    fs::write(
        directory.join("missing.jar"),
        jar("missing", "2.1.0", json!({})),
    )
    .unwrap();
    assert!(
        check_instance(root.path(), &target.instance, &server.downloader())
            .unwrap()
            .errors
            .is_empty()
    );
    fs::write(
        directory.join("duplicate.jar"),
        jar("a", "1.0.0", json!({})),
    )
    .unwrap();
    assert!(
        check_instance(root.path(), &target.instance, &server.downloader())
            .unwrap()
            .errors
            .iter()
            .any(|s| s.contains("重复"))
    );
}

#[test]
fn hash_identification_requires_explicit_adoption_preserves_disabled_files_and_rechecks_changes() {
    let root = temporary();
    let server = Server::new();
    let api = server.api();
    let target = target(root.path());
    let v = version(&server, "identified", "known", json!([]));
    let bytes = b"jar for known";
    server.json(
        "/api/version_files",
        json!({(hashes(bytes)["sha1"].clone()): v}),
    );
    let directory = target.instance.game_directory(root.path()).unwrap();
    fs::create_dir_all(directory.join("mods")).unwrap();
    let file = directory.join("mods/custom-name.jar.disabled");
    fs::write(&file, bytes).unwrap();
    let plan = identify(root.path(), &target, &api).unwrap();
    assert_eq!(plan.candidates.len(), 1);
    assert!(registry(&directory).unwrap().mods.is_empty());
    fs::write(&file, b"modified after identifying").unwrap();
    assert!(adopt(root.path(), &plan, &api.downloader).is_err());
    fs::write(&file, bytes).unwrap();
    adopt(root.path(), &plan, &api.downloader).unwrap();
    let managed = registry(&directory).unwrap();
    assert!(!managed.mods["identified"].enabled());
    assert_eq!(
        managed.mods["identified"].file.path,
        "mods/custom-name.jar.disabled"
    );
    assert_eq!(fs::read(&file).unwrap(), bytes);
}

#[test]
fn curseforge_restricted_files_use_verified_manual_cache_and_zip_manifests_normalize() {
    let root = temporary();
    let server = Server::new();
    let d = server.downloader();
    let api = CurseForge::with_base(d.clone(), format!("{}/cf", server.base));
    let bytes = jar("restricted", "1.0.0", json!({}));
    server.json(
        "/cf/mods/12",
        json!({"id":12,"name":"Restricted","classId":6,"allowModDistribution":false}),
    );
    let file = json!({"id":34,"modId":12,"displayName":"One","fileName":"restricted.jar","fileLength":bytes.len(),"downloadUrl":"https://edge.forgecdn.net/files/0/0/restricted.jar","hashes":[{"algo":1,"value":hashes(&bytes)["sha1"]}],"gameVersions":["test-1","Fabric"],"fileDate":"2026-01-01T00:00:00Z","releaseType":1,"dependencies":[{"modId":7,"relationType":3},{"modId":8,"relationType":5}]});
    server.json("/cf/mods/12/files/34", json!({"data":file}));
    // The API wraps both project and file records in data envelopes.
    server.json(
        "/cf/mods/12",
        json!({"data":{"id":12,"name":"Restricted","classId":6,"allowModDistribution":false}}),
    );
    server.json(
        "/cf/mods/12/files/34/changelog",
        json!({"data":"<p>Updated &amp; fixed</p>"}),
    );
    let version = api.version(12, 34).unwrap();
    assert!(version.files[0].url.is_empty());
    assert_eq!(version.loaders, ["fabric"]);
    assert!(version.changelog.contains("Updated & fixed"));
    assert_eq!(version.dependencies[0].dependency_type, "required");
    assert_eq!(version.dependencies[1].dependency_type, "incompatible");
    assert_eq!(
        curseforge_website(&version).unwrap(),
        "https://www.curseforge.com/projects/12/files/34"
    );
    let manifest = json!({"manifestType":"minecraftModpack","manifestVersion":1,"name":"CF pack","version":"1","minecraft":{"version":"test-1","modLoaders":[{"id":"fabric-0.20.1","primary":true}]},"overrides":"overrides","files":[{"projectID":12,"fileID":34,"required":true}]});
    let pack_path = root.path().join("cf.zip");
    fs::write(
        &pack_path,
        archive(&[
            ("manifest.json", serde_json::to_vec(&manifest).unwrap()),
            ("overrides/config/test.txt", b"cf overrides".to_vec()),
        ]),
    )
    .unwrap();
    let plan = pack::curseforge::inspect_with_client(root.path(), &pack_path, &d, &api).unwrap();
    assert_eq!(plan.manual_files.len(), 1);
    assert_eq!(plan.loader.unwrap().version, "0.20.1");
    assert!(
        download_mod_with_cache(&root.path().join("downloads"), root.path(), &version, &d).is_err()
    );
    let manual = root.path().join("browser-download.jar");
    fs::write(&manual, b"wrong file").unwrap();
    assert!(provide_manual(root.path(), &version, &manual, &d).is_err());
    fs::write(&manual, &bytes).unwrap();
    provide_manual(root.path(), &version, &manual, &d).unwrap();
    let output =
        download_mod_with_cache(&root.path().join("downloads"), root.path(), &version, &d).unwrap();
    assert_eq!(fs::read(output).unwrap(), bytes);
    let plan = pack::curseforge::inspect_with_client(root.path(), &pack_path, &d, &api).unwrap();
    assert!(plan.manual_files.is_empty());
    let mut mismatched = file;
    mismatched["id"] = json!(99);
    server.json("/cf/mods/12/files/34", json!({"data":mismatched}));
    assert!(api.file(12, 34).err().unwrap().contains("ID"));
}

#[test]
fn curseforge_key_is_only_sent_to_api_and_redirect_errors_do_not_expose_it() {
    let server = Server::new();
    let redirect = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", redirect.server_addr());
    let target = format!("{}/capture", server.base);
    let worker = std::thread::spawn(move || {
        let request = redirect
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert!(
            request
                .headers()
                .iter()
                .any(|h| h.field.equiv("x-api-key") && h.value.as_str() == "test-key-never-log")
        );
        let response = tiny_http::Response::empty(302)
            .with_header(tiny_http::Header::from_bytes("Location", target).unwrap());
        request.respond(response).unwrap();
    });
    let api = CurseForge::with_base(server.downloader(), base);
    let error = api.project(12).err().unwrap();
    assert!(error.contains("302"));
    assert!(!error.contains("test-key-never-log"));
    worker.join().unwrap();
    assert!(server.requests.lock().unwrap().is_empty());
}
