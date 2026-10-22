use super::tests::{Server, target, temporary, version};
use super::*;
use serde_json::json;
use std::{collections::BTreeSet, fs, sync::atomic::Ordering};

#[test]
fn removal_checks_dependents_and_preserves_worlds_config_and_unmanaged_files() {
    let root = temporary();
    let server = Server::new();
    let api = server.api();
    let target = target(root.path());
    version(&server, "dependency", "dep1", json!([]));
    let main = version(
        &server,
        "example",
        "v1",
        json!([{"project_id":"dependency","version_id":"dep1","dependency_type":"required"}]),
    );
    let plan = plan_mod(root.path(), target.clone(), main, &api).unwrap();
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    let directory = target.instance.directory(root.path()).unwrap();
    fs::create_dir_all(directory.join("saves/world")).unwrap();
    fs::create_dir_all(directory.join("config")).unwrap();
    fs::write(directory.join("saves/world/level.dat"), b"world").unwrap();
    fs::write(directory.join("config/user.json"), b"config").unwrap();
    fs::write(directory.join("mods/manual.jar"), b"manual").unwrap();
    let catalog = list_managed(root.path(), &target, &api.downloader).unwrap();
    assert_eq!(catalog.entries.len(), 2);
    assert_eq!(catalog.other_files, vec!["manual.jar"]);
    assert!(catalog.entries.iter().all(|m| m.intact));
    assert!(
        plan_removal(root.path(), &target, &BTreeSet::from(["dependency".into()]))
            .err()
            .unwrap()
            .contains("必需依赖")
    );
    let removal = plan_removal(
        root.path(),
        &target,
        &BTreeSet::from(["dependency".into(), "example".into()]),
    )
    .unwrap();
    assert_eq!(removal.unmanaged_count, 1);
    assert!(directory.join("mods/example-v1.jar").exists());
    remove_mods(root.path(), &removal, &api.downloader).unwrap();
    assert!(registry(&directory).unwrap().mods.is_empty());
    assert!(!directory.join("mods/example-v1.jar").exists());
    assert!(!directory.join("mods/dependency-dep1.jar").exists());
    assert_eq!(
        fs::read(directory.join("saves/world/level.dat")).unwrap(),
        b"world"
    );
    assert_eq!(
        fs::read(directory.join("config/user.json")).unwrap(),
        b"config"
    );
    assert_eq!(
        fs::read(directory.join("mods/manual.jar")).unwrap(),
        b"manual"
    );
}

#[test]
fn modified_files_changed_records_and_cancellation_prevent_removal() {
    let root = temporary();
    let server = Server::new();
    let api = server.api();
    let target = target(root.path());
    let main = version(&server, "example", "v1", json!([]));
    let plan = plan_mod(root.path(), target.clone(), main, &api).unwrap();
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    let directory = target.instance.directory(root.path()).unwrap();
    let selection = BTreeSet::from(["example".into()]);
    let removal = plan_removal(root.path(), &target, &selection).unwrap();
    let original = fs::read(directory.join("mods/example-v1.jar")).unwrap();
    fs::write(directory.join("mods/example-v1.jar"), b"user edited").unwrap();
    assert!(
        remove_mods(root.path(), &removal, &api.downloader)
            .unwrap_err()
            .contains("手动修改")
    );
    assert_eq!(registry(&directory).unwrap().mods.len(), 1);
    fs::write(directory.join("mods/example-v1.jar"), original).unwrap();
    api.downloader.cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        remove_mods(root.path(), &removal, &api.downloader).unwrap_err(),
        "操作已取消。"
    );
    assert!(directory.join("mods/example-v1.jar").exists());
    api.downloader.cancel.store(false, Ordering::Relaxed);
    let mut record = registry(&directory).unwrap();
    record.mods.get_mut("example").unwrap().version.name = "Changed".into();
    crate::store::write_json(&directory.join(REGISTRY), &record).unwrap();
    assert!(
        remove_mods(root.path(), &removal, &api.downloader)
            .unwrap_err()
            .contains("记录已改变")
    );
    assert!(directory.join("mods/example-v1.jar").exists());
}

#[test]
fn updates_choose_newer_compatible_releases_then_use_normal_install_plan() {
    let root = temporary();
    let server = Server::new();
    let api = server.api();
    let target = target(root.path());
    let current = version(&server, "example", "v1", json!([]));
    let plan = plan_mod(root.path(), target.clone(), current.clone(), &api).unwrap();
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    let mut next = version(&server, "example", "v2", json!([]));
    next.date_published = "2026-02-01T00:00:00Z".into();
    let mut beta = next.clone();
    beta.id = "beta".into();
    beta.version_type = "beta".into();
    beta.date_published = "2026-03-01T00:00:00Z".into();
    let mut wrong = next.clone();
    wrong.id = "wrong".into();
    wrong.game_versions = vec!["wrong-minecraft".into()];
    wrong.date_published = "2026-04-01T00:00:00Z".into();
    server.json(
        "/api/project/example/version",
        json!([beta, wrong, current, next]),
    );
    let catalog = list_managed(root.path(), &target, &api.downloader).unwrap();
    let report = check_updates(&catalog, &api).unwrap();
    assert!(report.warnings.is_empty());
    assert_eq!(report.available.len(), 1);
    assert_eq!(report.available[0].available.id, "v2");
    let plan = plan_mod(
        root.path(),
        target.clone(),
        report.available[0].available.clone(),
        &api,
    )
    .unwrap();
    let directory = target.instance.directory(root.path()).unwrap();
    assert!(directory.join("mods/example-v1.jar").exists());
    assert!(!directory.join("mods/example-v2.jar").exists());
    install_mods(root.path(), &plan, &api.downloader).unwrap();
    assert!(directory.join("mods/example-v2.jar").exists());
    assert!(!directory.join("mods/example-v1.jar").exists());
    assert!(
        check_updates(
            &list_managed(root.path(), &target, &api.downloader).unwrap(),
            &api
        )
        .unwrap()
        .available
        .is_empty()
    );
}
