use super::*;
use crate::instances::{self, Isolation};

fn temporary() -> tempfile::TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&parent).unwrap();
    tempfile::tempdir_in(parent).unwrap()
}
fn write_profile(root: &Path, id: &str) -> Value {
    let dir = root.join("versions").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(root.join("assets/indexes")).unwrap();
    fs::write(dir.join(format!("{id}.jar")), b"test client").unwrap();
    fs::write(root.join("assets/indexes/test.json"), br#"{"objects":{}}"#).unwrap();
    let value = json!({"id":id,"clientVersion":"1.20.1","type":"release","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","javaVersion":{"majorVersion":17},"downloads":{"client":{"url":"https://example.invalid/client","sha1":format!("{:x}",Sha1::digest(b"test client")),"size":11}},"assetIndex":{"id":"test"},"libraries":[{"name":"net.fabricmc:fabric-loader:0.19.5"}],"arguments":{"jvm":["-cp","${classpath}"],"game":["--gameDir","${game_directory}"]}});
    crate::store::write_json(&dir.join(format!("{id}.json")), &value).unwrap();
    value
}

#[test]
fn discovers_unicode_profiles_preserves_original_isolation_and_archive_never_moves_worlds() {
    let root = temporary();
    let id = "整合包 [Fabric 0.19.5]";
    write_profile(root.path(), id);
    let dir = root.path().join("versions").join(id);
    fs::create_dir_all(dir.join("saves/world")).unwrap();
    fs::write(dir.join("saves/world/level.dat"), b"world").unwrap();
    fs::create_dir_all(dir.join("PCL")).unwrap();
    fs::write(dir.join("PCL/Setup.ini"), "VersionArgumentIndieV2:True\n").unwrap();
    let before = fs::read(dir.join(format!("{id}.json"))).unwrap();
    let versions = crate::minecraft::installed_versions(root.path());
    assert_eq!(versions.len(), 1);
    assert!(versions[0].external);
    assert_eq!(versions[0].loader.as_ref().unwrap().minecraft, "1.20.1");
    let instance = instances::list(root.path()).pop().unwrap();
    assert_eq!(instance.name, id);
    assert_eq!(instance.isolation, Isolation::Version);
    assert_eq!(instance.game_directory(root.path()).unwrap(), dir);
    assert!(!root.path().join("instances").exists());
    let archive = instances::archive(root.path(), &instance).unwrap();
    assert!(instances::list(root.path()).is_empty());
    assert_eq!(
        fs::read(dir.join("saves/world/level.dat")).unwrap(),
        b"world"
    );
    let restored =
        instances::restore(root.path(), archive.file_name().unwrap().to_str().unwrap()).unwrap();
    assert_eq!(restored.game_directory(root.path()).unwrap(), dir);
    assert_eq!(instances::list(root.path()).len(), 1);
    assert_eq!(fs::read(dir.join(format!("{id}.json"))).unwrap(), before);
    fs::write(dir.join("PCL/Setup.ini"), "VersionArgumentIndieV2:False\n").unwrap();
    assert_eq!(initial_isolation(root.path(), id), Isolation::Shared);
}

#[test]
fn inherited_profiles_resolve_parent_client_and_local_maven_libraries_without_rewriting_json() {
    let root = temporary();
    let mut base = write_profile(root.path(), "base");
    base["logging"] = json!({});
    base["mainClass"] = "net.minecraft.client.main.Main".into();
    base["libraries"] = json!([]);
    crate::store::write_json(&root.path().join("versions/base/base.json"), &base).unwrap();
    let id = "子版本 with spaces";
    let dir = root.path().join("versions").join(id);
    fs::create_dir_all(&dir).unwrap();
    let child = json!({"id":id,"inheritsFrom":"base","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[{"name":"net.fabricmc:fabric-loader:0.19.5"}],"arguments":{"game":["--extra","test"]}});
    crate::store::write_json(&dir.join(format!("{id}.json")), &child).unwrap();
    let path = root
        .path()
        .join("libraries/net/fabricmc/fabric-loader/0.19.5/fabric-loader-0.19.5.jar");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"local loader").unwrap();
    let original = fs::read(dir.join(format!("{id}.json"))).unwrap();
    let (version, client) = prepare(root.path(), id, &Platform::current()).unwrap();
    assert_eq!(client, root.path().join("versions/base/base.jar"));
    assert_eq!(version.id, id);
    assert!(version.inherits_from.is_none());
    assert_eq!(version.libraries.len(), 1);
    assert_eq!(
        version.libraries[0]
            .downloads
            .artifact
            .as_ref()
            .unwrap()
            .sha1,
        format!("{:x}", Sha1::digest(b"local loader"))
    );
    assert_eq!(fs::read(dir.join(format!("{id}.json"))).unwrap(), original);
    fs::remove_file(path).unwrap();
    assert!(
        prepare(root.path(), id, &Platform::current())
            .unwrap_err()
            .contains("文件缺失")
    );
}

#[test]
fn incomplete_managed_installs_corrupt_markers_unsafe_names_and_cycles_are_not_imported() {
    let root = temporary();
    write_profile(root.path(), "base");
    let dir = root.path().join("versions/base");
    fs::write(dir.join(".nscl-installing"), b"NSCL").unwrap();
    assert!(crate::minecraft::installed_versions(root.path()).is_empty());
    fs::remove_file(dir.join(".nscl-installing")).unwrap();
    fs::write(dir.join("nscl-instance.json"), b"broken marker").unwrap();
    assert!(crate::minecraft::installed_versions(root.path()).is_empty());
    for id in ["../outside", "CON", "a/b", "a\\b", "C:outside", "trailing."] {
        assert!(crate::minecraft::validate_local_id(id).is_err(), "{id}");
    }
    let cycle = root.path().join("versions/cycle");
    fs::create_dir_all(&cycle).unwrap();
    crate::store::write_json(
        &cycle.join("cycle.json"),
        &json!({"id":"cycle","inheritsFrom":"cycle"}),
    )
    .unwrap();
    assert!(describe(root.path(), "cycle").unwrap_err().contains("循环"));
}

#[test]
#[ignore = "reads an explicitly supplied existing Minecraft root; never writes to it"]
fn live_existing_profile_scan_and_normalization() {
    let root =
        PathBuf::from(std::env::var("NSCL_EXISTING_MC_ROOT").expect("set NSCL_EXISTING_MC_ROOT"));
    let platform = Platform::current();
    let versions = crate::minecraft::installed_versions(&root);
    assert!(!versions.is_empty());
    let mut prepared = 0;
    for version in versions.iter().filter(|v| v.external) {
        match prepare(&root, &version.id, &platform) {
            Ok(_) => {
                prepared += 1;
                eprintln!("PARSED: {}", version.id);
            }
            Err(error) => eprintln!("MISSING/UNSUPPORTED: {}: {error}", version.id),
        }
    }
    eprintln!("Detected {}, normalized {}", versions.len(), prepared);
    assert!(prepared > 0);
}
