use super::*;

fn fixture() -> tempfile::TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&parent).unwrap();
    let root = tempfile::tempdir_in(parent).unwrap();
    let version = root.path().join("versions/1.20.1");
    fs::create_dir_all(&version).unwrap();
    fs::write(version.join("1.20.1.json"), b"{}").unwrap();
    crate::store::write_json(
        &version.join(CONFIG),
        &minecraft::InstalledVersion {
            external: false,
            id: "1.20.1".into(),
            kind: "release".into(),
            java_major: 17,
            platform: "test".into(),
            metadata_sha1: "test".into(),
            loader: None,
            extra_files: vec![],
        },
    )
    .unwrap();
    root
}

fn instance(root: &Path, id: &str, isolation: Isolation) -> Instance {
    let instance = Instance {
        id: id.into(),
        name: id.into(),
        version_id: "1.20.1".into(),
        isolation,
        ..Default::default()
    };
    save(root, &instance).unwrap();
    let saves = instance.directory(root).unwrap().join("saves");
    fs::create_dir_all(&saves).unwrap();
    fs::write(saves.join("world"), b"owned world").unwrap();
    instance
}

#[test]
fn deleting_isolated_instance_removes_owned_data_and_preserves_neighbours_and_runtime() {
    let root = fixture();
    let one = instance(root.path(), "pack-one", Isolation::Isolated);
    let two = instance(root.path(), "pack-two", Isolation::Isolated);
    fs::create_dir_all(root.path().join("saves")).unwrap();
    fs::write(root.path().join("saves/shared-world"), b"shared").unwrap();
    delete(root.path(), &one).unwrap();
    assert!(!one.directory(root.path()).unwrap().exists());
    assert!(!list(root.path()).iter().any(|i| i.id == one.id));
    assert!(
        two.directory(root.path())
            .unwrap()
            .join("saves/world")
            .is_file()
    );
    assert!(root.path().join("saves/shared-world").is_file());
    assert!(root.path().join("versions/1.20.1/1.20.1.json").is_file());
}

#[test]
fn deleted_default_does_not_reappear_after_refresh_and_runtime_can_be_reused() {
    let root = fixture();
    let default = list(root.path())
        .into_iter()
        .find(|i| i.id == "1.20.1")
        .unwrap();
    delete(root.path(), &default).unwrap();
    assert!(root.path().join("instances/1.20.1.deleted").is_file());
    assert!(!list(root.path()).iter().any(|i| i.id == default.id));
    let replacement = create(root.path(), "1.20.1", "New world").unwrap();
    assert!(list(root.path()).contains(&replacement));
}

#[test]
fn shared_and_version_game_directories_are_preserved() {
    let root = fixture();
    for isolation in [Isolation::Shared, Isolation::Version] {
        let one = instance(root.path(), "shared-one", isolation);
        let game = one.game_directory(root.path()).unwrap();
        fs::create_dir_all(game.join("mods")).unwrap();
        fs::write(game.join("mods/keep.jar"), b"keep").unwrap();
        delete(root.path(), &one).unwrap();
        assert!(game.join("mods/keep.jar").exists());
        assert!(!one.directory(root.path()).unwrap().exists());
    }
}

#[test]
fn archives_can_be_deleted_but_stale_instances_and_invalid_paths_are_rejected() {
    let root = fixture();
    let one = instance(root.path(), "1.20.1", Isolation::Isolated);
    let mut stale = one.clone();
    stale.name = "old name".into();
    assert!(delete(root.path(), &stale).is_err());
    assert!(
        one.directory(root.path())
            .unwrap()
            .join("saves/world")
            .exists()
    );
    let archive = archive(root.path(), &one).unwrap();
    let entry = archive.file_name().unwrap().to_str().unwrap();
    assert!(delete_archived(root.path(), "../versions", &one).is_err());
    assert!(delete_archived(root.path(), entry, &stale).is_err());
    delete_archived(root.path(), entry, &one).unwrap();
    assert!(!archive.exists());
    assert!(archived(root.path()).is_empty());
    assert!(!list(root.path()).iter().any(|i| i.id == one.id));
    assert!(root.path().join("versions/1.20.1/1.20.1.json").exists());
}

#[cfg(windows)]
#[test]
fn nested_junctions_are_rejected_before_any_instance_files_are_removed() {
    let root = fixture();
    let outside = fixture();
    let one = instance(root.path(), "pack-one", Isolation::Isolated);
    let link = one.directory(root.path()).unwrap().join("linked-world");
    let output = crate::minecraft::hidden_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:NSCL_TEST_JUNCTION -Value $env:NSCL_TEST_JUNCTION_TARGET | Out-Null"])
        .env("NSCL_TEST_JUNCTION", &link)
        .env("NSCL_TEST_JUNCTION_TARGET", outside.path())
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(delete(root.path(), &one).is_err());
    assert!(
        one.directory(root.path())
            .unwrap()
            .join("saves/world")
            .exists()
    );
    assert!(outside.path().join("versions/1.20.1/1.20.1.json").exists());
    fs::remove_dir(&link).unwrap();
}
