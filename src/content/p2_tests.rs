use super::*;
use crate::{game_data, instances::migration};
fn pack(root: &Path, game: &str, loader: &str, overrides: &[(&str, Vec<u8>)]) -> PackPlan {
    let index = json!({"formatVersion":1,"game":"minecraft","name":"P2 test","versionId":"1","dependencies":{"minecraft":game,(loader):"0.26.4"},"files":[]});
    let mut entries = vec![("modrinth.index.json", serde_json::to_vec(&index).unwrap())];
    entries.extend_from_slice(overrides);
    let path = root.join("p2.mrpack");
    fs::write(&path, archive(&entries)).unwrap();
    inspect_pack(root, &path).unwrap()
}
fn quilt(server: &Server) {
    vanilla(server);
    server.json(
        "/quilt-meta/v3/versions/loader/test-1",
        json!([{"loader":{"version":"0.26.4"}},{"loader":{"version":"0.27.0-beta.1"}}]),
    );
    server.json("/quilt-meta/v3/versions/loader/test-1/0.26.4/profile/json",json!({"id":"quilt-test","inheritsFrom":"test-1","mainClass":"org.quiltmc.loader.impl.launch.knot.KnotClient","libraries":[]}));
}
#[test]
fn quilt_catalog_pack_install_and_export_keep_the_quilt_component() {
    let root = temporary();
    let server = Server::new();
    quilt(&server);
    let d = server.downloader();
    let versions = crate::minecraft::loader_versions("test-1", LoaderKind::Quilt, &d).unwrap();
    assert_eq!(versions.len(), 2);
    assert!(versions.iter().any(|v| v.version == "0.26.4" && v.stable));
    let plan = pack(
        root.path(),
        "test-1",
        "quilt-loader",
        &[("overrides/config/a.txt", b"quilt".to_vec())],
    );
    let instance =
        install_pack(root.path(), &plan, &crate::store::Settings::default(), &d).unwrap();
    assert_eq!(instance.version_id, "test-1-quilt-0.26.4");
    let target = Target::from_instance(root.path(), &instance).unwrap();
    assert_eq!(target.loader, Some(LoaderKind::Quilt));
    let output = root.path().join("export.mrpack");
    export_pack(
        root.path(),
        &instance,
        &PackExportOptions::for_instance(&instance),
        &output,
        &d,
    )
    .unwrap();
    assert_eq!(
        inspect_pack(root.path(), &output)
            .unwrap()
            .loader
            .unwrap()
            .kind,
        LoaderKind::Quilt
    );
}
#[test]
fn quilt_native_metadata_uses_builtins_and_aliases_and_warns_about_conditional_constraints() {
    let root = temporary();
    let server = Server::new();
    quilt(&server);
    let d = server.downloader();
    let plan = pack(root.path(), "test-1", "quilt-loader", &[]);
    let instance =
        install_pack(root.path(), &plan, &crate::store::Settings::default(), &d).unwrap();
    let directory = instance.game_directory(root.path()).unwrap().join("mods");
    fs::create_dir_all(&directory).unwrap();
    let native = json!({"schema_version":1,"quilt_loader":{
        "id":"native","version":"1.0.0","metadata":{"name":"Native Quilt"},
        "depends":[{"id":"quilt_loader","versions":">=0.26.0"},"alias",{"id":"optional_missing","optional":true},
            ["one_of_a","one_of_b"],{"id":"conditional","unless":"other"},
            {"id":"complex","versions":{"any":[">1","<2"]}}],
        "provides":[{"id":"alias","version":"2.0.0"}]
    }});
    let dual = archive(&[
        ("quilt.mod.json", serde_json::to_vec(&native).unwrap()),
        (
            "fabric.mod.json",
            serde_json::to_vec(
                &json!({"id":"fabric_side","version":"1.0.0","depends":{"wrong_metadata":"*"}}),
            )
            .unwrap(),
        ),
    ]);
    let path = directory.join("native.jar");
    fs::write(&path, dual).unwrap();
    assert_eq!(metadata::read(&path).unwrap()[0].id, "fabric_side");
    let report = check_instance(root.path(), &instance, &d).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.warnings.iter().any(|s| s.contains("组合依赖")));
    assert!(report.warnings.iter().any(|s| s.contains("复杂版本约束")));
    let mut invalid = native;
    invalid["quilt_loader"]["depends"] = json!([{"id":"quilt_loader","versions":">=99.0.0"}]);
    fs::write(
        &path,
        archive(&[("quilt.mod.json", serde_json::to_vec(&invalid).unwrap())]),
    )
    .unwrap();
    assert_eq!(metadata::read(&path).unwrap()[0].id, "native");
    let report = check_instance(root.path(), &instance, &d).unwrap();
    assert!(
        report
            .errors
            .iter()
            .any(|s| s.contains("需要 quilt_loader"))
    );
}
#[test]
fn migration_copies_only_selected_content_and_runtime_and_rejects_stale_or_conflicting_targets() {
    let workspace = temporary();
    let source = workspace.path().join("source");
    let target = workspace.path().join("target");
    fs::create_dir_all(&source).unwrap();
    let server = Server::new();
    quilt(&server);
    let d = server.downloader();
    let plan = pack(
        &source,
        "test-1",
        "quilt-loader",
        &[
            ("overrides/config/a.txt", b"config".to_vec()),
            ("overrides/custom/a.txt", b"custom".to_vec()),
        ],
    );
    let instance = install_pack(&source, &plan, &crate::store::Settings::default(), &d).unwrap();
    let game = instance.game_directory(&source).unwrap();
    fs::create_dir_all(game.join("saves/world")).unwrap();
    fs::write(game.join("saves/world/level.dat"), b"world").unwrap();
    fs::create_dir_all(game.join("screenshots")).unwrap();
    let old_shot = game.join("screenshots/old.png");
    fs::write(&old_shot, b"image bytes").unwrap();
    let file = fs::OpenOptions::new().write(true).open(&old_shot).unwrap();
    file.set_times(
        fs::FileTimes::new()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000)),
    )
    .unwrap();
    drop(file);
    fs::create_dir_all(game.join("logs")).unwrap();
    fs::write(game.join("logs/private.log"), b"private log").unwrap();
    fs::write(source.join("nscl-accounts.json"), b"private accounts").unwrap();
    let preview = migration::inspect(&source, &target, &instance, &d).unwrap();
    assert!(preview.runtime_files > 0 && preview.content_files >= 3);
    let copied = migration::copy(&preview, &d).unwrap();
    let dest = copied.game_directory(&target).unwrap();
    assert_eq!(copied.isolation, instances::Isolation::Isolated);
    assert_ne!(copied.id, instance.id);
    assert_eq!(
        fs::read(dest.join("saves/world/level.dat")).unwrap(),
        b"world"
    );
    assert_eq!(fs::read(dest.join("custom/a.txt")).unwrap(), b"custom");
    assert_eq!(
        fs::metadata(dest.join("screenshots/old.png"))
            .unwrap()
            .modified()
            .unwrap(),
        fs::metadata(&old_shot).unwrap().modified().unwrap()
    );
    assert!(!dest.join("logs").exists());
    assert!(!target.join("nscl-accounts.json").exists());
    assert_eq!(
        Target::from_instance(&target, &copied).unwrap().loader,
        Some(LoaderKind::Quilt)
    );
    assert_eq!(
        fs::read(game.join("saves/world/level.dat")).unwrap(),
        b"world"
    );
    let second = workspace.path().join("stale");
    let cancelled = workspace.path().join("cancelled");
    let p = migration::inspect(&source, &cancelled, &instance, &d).unwrap();
    d.cancel.store(true, Ordering::Relaxed);
    assert!(migration::copy(&p, &d).is_err());
    assert!(!p.instance.directory(&cancelled).unwrap().exists());
    d.cancel.store(false, Ordering::Relaxed);
    let preview = migration::inspect(&source, &second, &instance, &d).unwrap();
    fs::write(game.join("config/a.txt"), b"changed after preview").unwrap();
    assert!(migration::copy(&preview, &d).is_err());
    assert!(!preview.instance.directory(&second).unwrap().exists());
    let preview = migration::inspect(&source, &target, &instance, &d).unwrap();
    let bad = target
        .join("versions")
        .join(&instance.version_id)
        .join(format!("{}.jar", instance.version_id));
    fs::write(&bad, b"target data").unwrap();
    assert!(
        migration::copy(&preview, &d)
            .err()
            .unwrap()
            .contains("不会覆盖")
    );
    assert_eq!(fs::read(bad).unwrap(), b"target data");
    assert!(migration::inspect(&source, &source.join("nested"), &instance, &d).is_err());
}
fn string(out: &mut Vec<u8>, s: &str) {
    out.extend((s.len() as u16).to_be_bytes());
    out.extend(s.as_bytes());
}
fn named(out: &mut Vec<u8>, tag: u8, name: &str) {
    out.push(tag);
    string(out, name);
}
#[test]
fn world_metadata_and_screenshot_previews_are_bounded_and_do_not_write_game_files() {
    let root = temporary();
    let server = Server::new();
    let target = target(root.path());
    let game = target.instance.game_directory(root.path()).unwrap();
    fs::create_dir_all(game.join("saves/folder name")).unwrap();
    let mut nbt = vec![10, 0, 0];
    named(&mut nbt, 10, "Data");
    named(&mut nbt, 8, "LevelName");
    string(&mut nbt, "我的世界");
    named(&mut nbt, 4, "LastPlayed");
    nbt.extend(1_700_000_000_000i64.to_be_bytes());
    named(&mut nbt, 3, "GameType");
    nbt.extend(1i32.to_be_bytes());
    named(&mut nbt, 10, "Version");
    named(&mut nbt, 8, "Name");
    string(&mut nbt, "1.20.1");
    nbt.extend([0, 0, 0]);
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&nbt).unwrap();
    let bytes = gzip.finish().unwrap();
    let path = game.join("saves/folder name/level.dat");
    fs::write(&path, &bytes).unwrap();
    fs::create_dir_all(game.join("screenshots")).unwrap();
    image::RgbaImage::from_pixel(640, 360, image::Rgba([30, 60, 80, 255]))
        .save(game.join("screenshots/test.png"))
        .unwrap();
    let catalog = game_data::list(root.path(), &target.instance, &server.downloader()).unwrap();
    let world = &catalog.worlds[0];
    assert_eq!(world.name, "我的世界");
    assert_eq!(world.version, "1.20.1");
    assert_eq!(world.game_mode, "创造");
    assert_eq!(world.last_played, 1_700_000_000);
    assert!(world.warning.is_none());
    let image = game_data::thumbnail(&game, &catalog.screenshots[0]).unwrap();
    assert_eq!((image.width(), image.height()), (320, 180));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::write(&path, b"invalid").unwrap();
    assert!(
        game_data::list(root.path(), &target.instance, &server.downloader())
            .unwrap()
            .worlds[0]
            .warning
            .is_some()
    );
    assert!(game_data::checked_path(&game, "screenshots/../outside.png").is_err());
}
#[test]
fn instance_groups_and_pins_roundtrip_and_pins_take_priority_over_recent_usage() {
    let root = temporary();
    let target = target(root.path());
    let mut a = target.instance.clone();
    a.group = "生存".into();
    a.pinned = true;
    instances::save(root.path(), &a).unwrap();
    let mut b = instances::create(root.path(), &a.version_id, "Recent").unwrap();
    b.last_played = Some(u64::MAX - 1);
    instances::save(root.path(), &b).unwrap();
    let list = instances::list(root.path());
    assert_eq!(list[0].id, a.id);
    assert_eq!(list[0].group, "生存");
    a.group = "bad\nname".into();
    assert!(instances::save(root.path(), &a).is_err());
}
#[test]
fn optifine_import_requires_forge_matching_game_filename_and_explicit_forge_version() {
    let root = temporary();
    let server = Server::new();
    let mut target = target(root.path());
    let path = root.path().join("OptiFine_test-1_HD_U_X.jar");
    fs::write(
        &path,
        archive(&[("optifine/Installer.class", b"fixture".to_vec())]),
    )
    .unwrap();
    assert!(
        import_optifine(
            root.path(),
            &target.instance,
            &path,
            "0.20.1",
            &server.downloader()
        )
        .is_err()
    );
    let meta = root
        .path()
        .join("versions")
        .join(&target.instance.version_id)
        .join("nscl-instance.json");
    let mut installed: InstalledVersion =
        serde_json::from_slice(&fs::read(&meta).unwrap()).unwrap();
    installed.loader.as_mut().unwrap().kind = LoaderKind::Forge;
    crate::store::write_json(&meta, &installed).unwrap();
    target.loader = Some(LoaderKind::Forge);
    assert!(
        import_optifine(
            root.path(),
            &target.instance,
            &path,
            "different",
            &server.downloader()
        )
        .is_err()
    );
    import_optifine(
        root.path(),
        &target.instance,
        &path,
        "0.20.1",
        &server.downloader(),
    )
    .unwrap();
    assert!(
        target
            .instance
            .game_directory(root.path())
            .unwrap()
            .join("mods/OptiFine_test-1_HD_U_X.jar")
            .exists()
    );
}
