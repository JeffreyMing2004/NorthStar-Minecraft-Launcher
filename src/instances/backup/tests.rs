use super::*;
use std::sync::{Arc, atomic::AtomicBool};

fn fixture(isolation: Isolation) -> (tempfile::TempDir, Instance, Downloader) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&parent).unwrap();
    let root = tempfile::tempdir_in(parent).unwrap();
    let instance = Instance {
        id: "test".into(),
        name: "Test".into(),
        version_id: "1.20.1".into(),
        isolation,
        ..Default::default()
    };
    super::super::save(root.path(), &instance).unwrap();
    let d = Downloader::new(
        Default::default(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(|_| {}),
    )
    .unwrap();
    (root, instance, d)
}
fn write(root: &Path, path: &str, value: &str) {
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, value).unwrap();
}
#[test]
fn full_restore_replaces_content_and_preserves_pre_restore_recovery() {
    let (root, instance, d) = fixture(Isolation::Isolated);
    let game = instance.game_directory(root.path()).unwrap();
    let backups = root.path().join("backups");
    write(&game, "mods/a.jar", "one");
    write(&game, "saves/world/level.dat", "world-one");
    fs::create_dir_all(game.join("config/empty")).unwrap();
    let snapshot = create(root.path(), &instance, &backups, Scope::Instance, false, &d).unwrap();
    write(&game, "mods/a.jar", "two");
    write(&game, "mods/added.jar", "added");
    write(&game, "saves/world/level.dat", "world-two");
    restore(root.path(), &instance, &backups, &snapshot.id, &d).unwrap();
    assert_eq!(fs::read_to_string(game.join("mods/a.jar")).unwrap(), "one");
    assert!(!game.join("mods/added.jar").exists());
    assert!(game.join("config/empty").is_dir());
    let recovery = list(&backups, &instance)
        .unwrap()
        .into_iter()
        .find(|s| s.recovery)
        .unwrap();
    restore(root.path(), &instance, &backups, &recovery.id, &d).unwrap();
    assert_eq!(fs::read_to_string(game.join("mods/a.jar")).unwrap(), "two");
    assert_eq!(
        fs::read_to_string(game.join("saves/world/level.dat")).unwrap(),
        "world-two"
    );
}
#[test]
fn shared_save_snapshot_only_restores_saves_and_clone_is_isolated() {
    let (root, instance, d) = fixture(Isolation::Shared);
    let backups = root.path().join("backups");
    write(root.path(), "saves/world/level.dat", "one");
    write(root.path(), "mods/a.jar", "mod-one");
    write(root.path(), "screenshots/a.png", "shot");
    write(root.path(), "servers.dat", "servers");
    let s = create(root.path(), &instance, &backups, Scope::Saves, false, &d).unwrap();
    write(root.path(), "saves/world/level.dat", "two");
    write(root.path(), "mods/a.jar", "mod-two");
    restore(root.path(), &instance, &backups, &s.id, &d).unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join("mods/a.jar")).unwrap(),
        "mod-two"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("saves/world/level.dat")).unwrap(),
        "one"
    );
    let clone = clone_instance(
        root.path(),
        &instance,
        &CloneOptions {
            name: "Clone".into(),
            saves: true,
            screenshots: false,
            servers: false,
        },
        &d,
    )
    .unwrap();
    let cloned = clone.game_directory(root.path()).unwrap();
    assert_eq!(clone.isolation, Isolation::Isolated);
    assert!(!cloned.join("screenshots").exists());
    assert!(!cloned.join("servers.dat").exists());
    write(&cloned, "saves/world/level.dat", "independent");
    assert_eq!(
        fs::read_to_string(root.path().join("saves/world/level.dat")).unwrap(),
        "one"
    );
}
#[test]
fn corrupt_backup_and_traversal_never_modify_source() {
    let (root, instance, d) = fixture(Isolation::Isolated);
    let game = instance.game_directory(root.path()).unwrap();
    let backups = root.path().join("backups");
    write(&game, "mods/a.jar", "original");
    let mut s = create(root.path(), &instance, &backups, Scope::Instance, false, &d).unwrap();
    write(&backups.join(&s.id).join("data"), "mods/a.jar", "corrupt!");
    assert!(restore(root.path(), &instance, &backups, &s.id, &d).is_err());
    assert_eq!(list(&backups, &instance).unwrap().len(), 1);
    assert_eq!(
        fs::read_to_string(game.join("mods/a.jar")).unwrap(),
        "original"
    );
    s.entries[0].path = "../outside".into();
    crate::store::write_json(&backups.join(&s.id).join("snapshot.json"), &s).unwrap();
    assert!(restore(root.path(), &instance, &backups, &s.id, &d).is_err());
}
#[test]
fn publish_failure_rolls_back_already_replaced_roots() {
    let (root, instance, d) = fixture(Isolation::Isolated);
    let game = instance.game_directory(root.path()).unwrap();
    let backups = root.path().join("backups");
    write(&game, "mods/a.jar", "old");
    let s = create(root.path(), &instance, &backups, Scope::Instance, false, &d).unwrap();
    write(&game, "mods/a.jar", "current");
    let error = restore_inner(root.path(), &instance, &backups, &s.id, &d, |p| {
        if p == "config" {
            Err("simulated publication failure".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(error.contains("已回退"));
    assert_eq!(
        fs::read_to_string(game.join("mods/a.jar")).unwrap(),
        "current"
    );
    assert!(
        list(&backups, &instance)
            .unwrap()
            .iter()
            .any(|s| s.recovery)
    );
}
#[test]
fn cancellation_does_not_publish_and_store_cannot_be_inside_saves() {
    let (root, instance, _) = fixture(Isolation::Isolated);
    let game = instance.game_directory(root.path()).unwrap();
    write(&game, "saves/world/level.dat", "data");
    let cancel = Arc::new(AtomicBool::new(true));
    let d = Downloader::new(Default::default(), cancel, Arc::new(|_| {})).unwrap();
    let backups = root.path().join("backups");
    assert!(create(root.path(), &instance, &backups, Scope::Instance, false, &d).is_err());
    assert!(list(&backups, &instance).unwrap().is_empty());
    let d = Downloader::new(
        Default::default(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(|_| {}),
    )
    .unwrap();
    assert!(
        create(
            root.path(),
            &instance,
            &game.join("saves/backups"),
            Scope::Instance,
            false,
            &d
        )
        .is_err()
    );
}

#[test]
fn cancellation_after_staging_removes_partial_snapshot() {
    let (root, instance, _) = fixture(Isolation::Isolated);
    let game = instance.game_directory(root.path()).unwrap();
    write(&game, "saves/world/level.dat", "original");
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let d = Downloader::new(
        Default::default(),
        cancel,
        Arc::new(move |p| {
            if p.stage == "复制并校验备份内容" {
                signal.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }),
    )
    .unwrap();
    let destination = root.path().join("backups");
    assert!(
        create(
            root.path(),
            &instance,
            &destination,
            Scope::Saves,
            false,
            &d
        )
        .is_err()
    );
    assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
    assert_eq!(
        fs::read_to_string(game.join("saves/world/level.dat")).unwrap(),
        "original"
    );
}
