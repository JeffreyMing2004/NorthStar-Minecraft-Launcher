use super::*;
use crate::content::{
    tests::{Server, target, temporary, version},
    *,
};
use serde_json::json;
use std::{
    io::Write,
    sync::{Arc, atomic::AtomicBool},
};

fn downloader() -> Downloader {
    Downloader::new(
        Default::default(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(|_| {}),
    )
    .unwrap()
}
fn zip(path: &Path, files: &[(&str, &[u8])]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in files {
        archive
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap();
}

#[test]
fn local_files_can_be_imported_disabled_and_recycled_without_overwriting() {
    let root = temporary();
    let target = target(root.path());
    let d = downloader();
    let source = root.path().join("imports/pack.zip");
    zip(
        &source,
        &[(
            "pack.mcmeta",
            br#"{"pack":{"pack_format":15,"description":"test"}}"#,
        )],
    );
    import_local(
        root.path(),
        &target.instance,
        LocalKind::ResourcePacks,
        std::slice::from_ref(&source),
        &d,
    )
    .unwrap();
    assert!(source.exists());
    let catalog = list_local(root.path(), &target.instance, LocalKind::ResourcePacks, &d).unwrap();
    assert!(catalog.entries[0].enabled);
    assert!(
        import_local(
            root.path(),
            &target.instance,
            LocalKind::ResourcePacks,
            std::slice::from_ref(&source),
            &d
        )
        .is_err()
    );
    change_local(
        root.path(),
        &catalog,
        &catalog.entries[0],
        LocalAction::Disable,
        &d,
    )
    .unwrap();
    let catalog = list_local(root.path(), &target.instance, LocalKind::ResourcePacks, &d).unwrap();
    assert!(!catalog.entries[0].enabled);
    assert!(
        !catalog
            .game_directory
            .join("resourcepacks/pack.zip")
            .exists()
    );
    change_local(
        root.path(),
        &catalog,
        &catalog.entries[0],
        LocalAction::Enable,
        &d,
    )
    .unwrap();
    let catalog = list_local(root.path(), &target.instance, LocalKind::ResourcePacks, &d).unwrap();
    change_local(
        root.path(),
        &catalog,
        &catalog.entries[0],
        LocalAction::Remove,
        &d,
    )
    .unwrap();
    assert!(
        list_local(root.path(), &target.instance, LocalKind::ResourcePacks, &d)
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(
        catalog
            .game_directory
            .join(".nscl/trash/resourcepacks")
            .is_dir()
    );
}

#[test]
fn folder_packs_keep_their_files_and_pack_tracking_when_disabled() {
    let root = temporary();
    let target = target(root.path());
    let d = downloader();
    let game = target.instance.game_directory(root.path()).unwrap();
    let pack = game.join("resourcepacks/Folder pack");
    fs::create_dir_all(&pack).unwrap();
    fs::write(pack.join("pack.mcmeta"), b"{}").unwrap();
    let file = files::record(
        &pack.join("pack.mcmeta"),
        "resourcepacks/Folder pack/pack.mcmeta".into(),
    )
    .unwrap();
    let record = Registry {
        mods: Default::default(),
        pack: Some(PackRecord {
            name: "Pack".into(),
            version: "1".into(),
            minecraft: target.minecraft.clone(),
            loader: None,
            files: vec![file],
        }),
    };
    crate::store::write_json(&game.join(REGISTRY), &record).unwrap();
    let catalog = list_local(root.path(), &target.instance, LocalKind::ResourcePacks, &d).unwrap();
    change_local(
        root.path(),
        &catalog,
        &catalog.entries[0],
        LocalAction::Disable,
        &d,
    )
    .unwrap();
    let disabled = game.join(".nscl/disabled/resourcepacks/Folder pack/pack.mcmeta");
    assert_eq!(fs::read(disabled).unwrap(), b"{}");
    assert!(
        registry(&game).unwrap().pack.unwrap().files[0]
            .path
            .starts_with(".nscl/disabled/")
    );
}

#[test]
fn managed_mod_dependencies_and_disabled_updates_are_preserved() {
    let root = temporary();
    let server = Server::new();
    let api = server.api();
    let target = target(root.path());
    version(&server, "base", "b1", json!([]));
    let addon = version(
        &server,
        "addon",
        "a1",
        json!([{"dependency_type":"required","project_id":"base"}]),
    );
    let plan = plan_mod(root.path(), target.clone(), addon, &api).unwrap();
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    let catalog = list_local(
        root.path(),
        &target.instance,
        LocalKind::Mods,
        &api.downloader,
    )
    .unwrap();
    let base = catalog
        .entries
        .iter()
        .find(|e| e.project.as_deref() == Some("base"))
        .unwrap();
    assert!(
        change_local(
            root.path(),
            &catalog,
            base,
            LocalAction::Disable,
            &api.downloader
        )
        .is_err()
    );
    let addon = catalog
        .entries
        .iter()
        .find(|e| e.project.as_deref() == Some("addon"))
        .unwrap();
    change_local(
        root.path(),
        &catalog,
        addon,
        LocalAction::Disable,
        &api.downloader,
    )
    .unwrap();
    let new = version(
        &server,
        "addon",
        "a2",
        json!([{"dependency_type":"required","project_id":"base"}]),
    );
    let plan = plan_mod(root.path(), target.clone(), new, &api).unwrap();
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    let record = registry(&target.instance.game_directory(root.path()).unwrap()).unwrap();
    assert!(!record.mods["addon"].enabled());
    assert!(record.mods["addon"].file.path.ends_with("a2.jar.disabled"));
    assert!(
        check_instance(root.path(), &target.instance, &api.downloader)
            .unwrap()
            .errors
            .is_empty()
    );
}

#[test]
fn imports_validate_type_stale_context_and_entire_batch_before_publishing() {
    let root = temporary();
    let mut target = target(root.path());
    let d = downloader();
    let one = root.path().join("imports/one.zip");
    let bad = root.path().join("imports/bad.zip");
    zip(&one, &[("shaders/test.vsh", b"shader")]);
    zip(&bad, &[("readme.txt", b"not a shader")]);
    assert!(
        import_local(
            root.path(),
            &target.instance,
            LocalKind::Shaders,
            &[one.clone(), bad],
            &d
        )
        .is_err()
    );
    let game = target.instance.game_directory(root.path()).unwrap();
    assert!(!game.join("shaderpacks/one.zip").exists());
    import_local(
        root.path(),
        &target.instance,
        LocalKind::Shaders,
        &[one],
        &d,
    )
    .unwrap();
    let catalog = list_local(root.path(), &target.instance, LocalKind::Shaders, &d).unwrap();
    target.instance.isolation = instances::Isolation::Shared;
    instances::save(root.path(), &target.instance).unwrap();
    assert!(
        change_local(
            root.path(),
            &catalog,
            &catalog.entries[0],
            LocalAction::Remove,
            &d
        )
        .is_err()
    );
    assert!(game.join("shaderpacks/one.zip").exists());
}

#[test]
fn a_file_created_by_another_program_during_import_is_not_replaced() {
    let root = temporary();
    let target = target(root.path());
    let source = root.path().join("imports/pack.zip");
    zip(&source, &[("pack.mcmeta", b"{}")]);
    let destination = target
        .instance
        .game_directory(root.path())
        .unwrap()
        .join("resourcepacks/pack.zip");
    let concurrent = destination.clone();
    let d = Downloader::new(
        Default::default(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(move |progress| {
            if progress.stage == "发布本地资源" {
                fs::create_dir_all(concurrent.parent().unwrap()).unwrap();
                fs::write(&concurrent, b"created by another program").unwrap();
            }
        }),
    )
    .unwrap();
    assert!(
        import_local(
            root.path(),
            &target.instance,
            LocalKind::ResourcePacks,
            &[source],
            &d
        )
        .is_err()
    );
    assert_eq!(
        fs::read(destination).unwrap(),
        b"created by another program"
    );
}

#[test]
fn missing_registered_mods_remain_visible_and_their_records_can_be_removed() {
    let root = temporary();
    let server = Server::new();
    let api = server.api();
    let target = target(root.path());
    let selected = version(&server, "example", "v1", json!([]));
    let plan = plan_mod(root.path(), target.clone(), selected, &api).unwrap();
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    let game = target.instance.game_directory(root.path()).unwrap();
    fs::remove_file(game.join("mods/example-v1.jar")).unwrap();
    let catalog = list_local(
        root.path(),
        &target.instance,
        LocalKind::Mods,
        &api.downloader,
    )
    .unwrap();
    assert_eq!(catalog.entries.len(), 1);
    assert!(catalog.entries[0].missing);
    change_local(
        root.path(),
        &catalog,
        &catalog.entries[0],
        LocalAction::Remove,
        &api.downloader,
    )
    .unwrap();
    assert!(registry(&game).unwrap().mods.is_empty());
    assert!(!game.join("mods/example-v1.jar").exists());
}
