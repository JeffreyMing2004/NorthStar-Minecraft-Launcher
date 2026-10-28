use super::*;
use crate::{
    content::{
        inspect_pack, install_pack,
        tests::{Server, target, temporary},
    },
    instances::{self, Isolation},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn progress() -> Downloader {
    Downloader::new(
        Default::default(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(|_| {}),
    )
    .unwrap()
}

fn write(directory: &Path, relative: &str, bytes: &[u8]) {
    let path = directory.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn entries(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (entry.name().to_owned(), bytes)
        })
        .collect()
}

#[test]
fn export_defaults_include_client_content_and_exclude_private_and_runtime_files() {
    let root = temporary();
    let target = target(root.path());
    let directory = target.instance.game_directory(root.path()).unwrap();
    for relative in [
        "mods/enabled.jar",
        "mods/disabled.jar.disabled",
        "config/中文.json",
        "kubejs/startup_scripts/test.js",
        "resourcepacks/test.zip",
        "shaderpacks/test.zip",
    ] {
        write(&directory, relative, relative.as_bytes());
    }
    for relative in [
        "saves/World/level.dat",
        "options.txt",
        "servers.dat",
        "logs/latest.log",
        "screenshots/test.png",
        "accounts.json",
        ".nscl-content.json",
        ".nscl/private",
        "versions/game.jar",
        "libraries/test.jar",
    ] {
        write(&directory, relative, b"private");
    }
    let output = root.path().join("整合包.mrpack");
    export_pack(
        root.path(),
        &target.instance,
        &PackExportOptions::for_instance(&target.instance),
        &output,
        &progress(),
    )
    .unwrap();
    let files = entries(&output);
    assert_eq!(files.len(), 7);
    assert_eq!(
        files["client-overrides/config/中文.json"],
        "config/中文.json".as_bytes()
    );
    assert!(files.contains_key("client-overrides/mods/disabled.jar.disabled"));
    let index: serde_json::Value = serde_json::from_slice(&files["modrinth.index.json"]).unwrap();
    assert_eq!(
        index["dependencies"],
        serde_json::json!({"minecraft":"test-1","fabric-loader":"0.20.1"})
    );
    let plan = inspect_pack(root.path(), &output).unwrap();
    assert_eq!(plan.override_count(), 6);
    assert_eq!(plan.download_count(), 0);
    assert_eq!(plan.loader.unwrap().kind, LoaderKind::Fabric);
}

#[test]
fn export_shared_directory_and_opt_in_files_can_be_installed_as_an_isolated_pack() {
    let root = temporary();
    let mut instance = target(root.path()).instance;
    instance.isolation = Isolation::Shared;
    instances::save(root.path(), &instance).unwrap();
    write(root.path(), "mods/client.jar", b"mod");
    write(root.path(), "config/test.txt", b"configuration");
    write(root.path(), "saves/My World/level.dat", b"world");
    write(root.path(), "options.txt", b"settings");
    write(root.path(), "servers.dat", b"servers");
    let mut options = PackExportOptions::for_instance(&instance);
    options.include_saves = true;
    options.include_settings = true;
    options.name = "Round trip".into();
    options.version = "2.3".into();
    options.summary = "A client pack".into();
    let output = root.path().join("roundtrip.mrpack");
    export_pack(root.path(), &instance, &options, &output, &progress()).unwrap();
    let plan = inspect_pack(root.path(), &output).unwrap();
    assert_eq!(plan.override_count(), 5);
    assert_eq!(plan.name, "Round trip");
    assert_eq!(plan.version, "2.3");
    let server = Server::new();
    crate::content::tests::vanilla(&server);
    let api = server.api();
    let restored = install_pack(
        root.path(),
        &plan,
        &crate::store::Settings::default(),
        &api.downloader,
    )
    .unwrap();
    assert_eq!(restored.isolation, Isolation::Isolated);
    assert_ne!(restored.id, instance.id);
    let restored_dir = restored.game_directory(root.path()).unwrap();
    for (relative, bytes) in [
        ("mods/client.jar", b"mod".as_slice()),
        ("config/test.txt", b"configuration"),
        ("saves/My World/level.dat", b"world"),
        ("options.txt", b"settings"),
        ("servers.dat", b"servers"),
    ] {
        assert_eq!(fs::read(restored_dir.join(relative)).unwrap(), bytes);
    }
    assert_eq!(
        fs::read(root.path().join("saves/My World/level.dat")).unwrap(),
        b"world"
    );
}

#[test]
fn export_records_all_supported_loader_dependencies_and_vanilla() {
    for kind in [
        None,
        Some(LoaderKind::Fabric),
        Some(LoaderKind::Forge),
        Some(LoaderKind::NeoForge),
    ] {
        let root = temporary();
        let target = target(root.path());
        let directory = root
            .path()
            .join("versions")
            .join(&target.instance.version_id);
        let path = directory.join("nscl-instance.json");
        let mut version: minecraft::InstalledVersion =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        version.loader = kind.map(|kind| minecraft::LoaderVersion {
            kind,
            minecraft: "test-1".into(),
            version: "1.2.3".into(),
            stable: true,
        });
        crate::store::write_json(&path, &version).unwrap();
        let output = root.path().join("loader.mrpack");
        export_pack(
            root.path(),
            &target.instance,
            &PackExportOptions::for_instance(&target.instance),
            &output,
            &progress(),
        )
        .unwrap();
        let files = entries(&output);
        let index: serde_json::Value =
            serde_json::from_slice(&files["modrinth.index.json"]).unwrap();
        assert_eq!(
            index["dependencies"].as_object().unwrap().len(),
            if kind.is_some() { 2 } else { 1 }
        );
        if let Some(kind) = kind {
            let key = if kind == LoaderKind::Fabric {
                "fabric-loader"
            } else {
                kind.key()
            };
            assert_eq!(index["dependencies"][key], "1.2.3");
        }
        inspect_pack(root.path(), &output).unwrap();
    }
}

#[test]
fn export_rejects_existing_output_nested_output_bad_metadata_and_cancellation() {
    let root = temporary();
    let target = target(root.path());
    let directory = target.instance.game_directory(root.path()).unwrap();
    write(&directory, "config/test.txt", b"config");
    let output = root.path().join("exists.mrpack");
    fs::write(&output, b"keep").unwrap();
    let options = PackExportOptions::for_instance(&target.instance);
    assert!(
        export_pack(
            root.path(),
            &target.instance,
            &options,
            &output,
            &progress()
        )
        .unwrap_err()
        .contains("已存在")
    );
    assert_eq!(fs::read(&output).unwrap(), b"keep");
    assert!(
        export_pack(
            root.path(),
            &target.instance,
            &options,
            &directory.join("config/nested.mrpack"),
            &progress()
        )
        .unwrap_err()
        .contains("之外")
    );
    let mut invalid = options.clone();
    invalid.name = " ".into();
    assert!(
        export_pack(
            root.path(),
            &target.instance,
            &invalid,
            &root.path().join("invalid.mrpack"),
            &progress()
        )
        .is_err()
    );
    for stage in ["压缩整合包文件", "保存整合包"] {
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let d = Downloader::new(
            Default::default(),
            cancelled,
            Arc::new(move |p| {
                if p.stage == stage {
                    flag.store(true, Ordering::Relaxed);
                }
            }),
        )
        .unwrap();
        let output = root.path().join("cancelled.mrpack");
        assert_eq!(
            export_pack(root.path(), &target.instance, &options, &output, &d).unwrap_err(),
            "操作已取消。"
        );
        assert!(!output.exists());
        assert!(!fs::read_dir(root.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".nscl-export-")
        }));
    }
}

#[test]
fn export_rejects_oversized_files_and_vanilla_mod_jars() {
    let root = temporary();
    let target = target(root.path());
    let directory = target.instance.game_directory(root.path()).unwrap();
    write(&directory, "config/large.bin", b"");
    fs::File::options()
        .write(true)
        .open(directory.join("config/large.bin"))
        .unwrap()
        .set_len(files::MAX_FILE + 1)
        .unwrap();
    let output = root.path().join("bad.mrpack");
    let options = PackExportOptions::for_instance(&target.instance);
    assert!(
        export_pack(
            root.path(),
            &target.instance,
            &options,
            &output,
            &progress()
        )
        .unwrap_err()
        .contains("512 MiB")
    );
    assert!(!output.exists());
    fs::remove_file(directory.join("config/large.bin")).unwrap();
    let metadata = root
        .path()
        .join("versions")
        .join(&target.instance.version_id)
        .join("nscl-instance.json");
    let mut installed: minecraft::InstalledVersion =
        serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
    installed.loader = None;
    crate::store::write_json(&metadata, &installed).unwrap();
    write(&directory, "mods/client.jar", b"mod");
    assert!(
        export_pack(
            root.path(),
            &target.instance,
            &options,
            &output,
            &progress()
        )
        .unwrap_err()
        .contains("没有支持的加载器")
    );
    assert!(!output.exists());
}

#[test]
fn export_failure_after_scanning_cleans_temporary_files_and_preserves_a_racing_output() {
    let root = temporary();
    let target = target(root.path());
    let directory = target.instance.game_directory(root.path()).unwrap();
    write(&directory, "config/test.txt", b"config");
    let output = root.path().join("race.mrpack");
    let racing_output = output.clone();
    let d = Downloader::new(
        Default::default(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(move |p| {
            if p.stage == "保存整合包" {
                fs::write(&racing_output, b"keep existing").unwrap();
            }
        }),
    )
    .unwrap();
    let options = PackExportOptions::for_instance(&target.instance);
    assert!(export_pack(root.path(), &target.instance, &options, &output, &d).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"keep existing");
    let source = directory.join("config/test.txt");
    let d = Downloader::new(
        Default::default(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(move |p| {
            if p.stage == "压缩整合包文件" {
                fs::remove_file(&source).unwrap();
            }
        }),
    )
    .unwrap();
    assert!(
        export_pack(
            root.path(),
            &target.instance,
            &options,
            &root.path().join("removed.mrpack"),
            &d
        )
        .is_err()
    );
    assert!(!root.path().join("removed.mrpack").exists());
    assert!(!fs::read_dir(root.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".nscl-export-")
    }));
}

#[test]
fn export_external_renamed_vanilla_resolves_minecraft_and_reads_version_directory() {
    let root = temporary();
    let base = serde_json::json!({"id":"1.20.1","mainClass":"net.minecraft.client.main.Main","downloads":{"client":{}},"assetIndex":{"id":"test-assets"},"libraries":[]});
    write(
        root.path(),
        "versions/1.20.1/1.20.1.json",
        &serde_json::to_vec(&base).unwrap(),
    );
    write(
        root.path(),
        "versions/Renamed/Renamed.json",
        br#"{"id":"Renamed","inheritsFrom":"1.20.1"}"#,
    );
    write(
        root.path(),
        "versions/Renamed/config/test.txt",
        b"version config",
    );
    let installed = minecraft::installed_versions(root.path());
    assert!(installed.iter().any(|v| v.id == "Renamed"));
    let instance = Instance {
        id: "external-export-test".into(),
        name: "Renamed vanilla".into(),
        version_id: "Renamed".into(),
        isolation: Isolation::Version,
        ..Default::default()
    };
    let output = root.path().join("external.mrpack");
    export_pack(
        root.path(),
        &instance,
        &PackExportOptions::for_instance(&instance),
        &output,
        &progress(),
    )
    .unwrap();
    let plan = inspect_pack(root.path(), &output).unwrap();
    assert_eq!(plan.minecraft, "1.20.1");
    assert!(plan.loader.is_none());
    assert_eq!(
        entries(&output)["client-overrides/config/test.txt"],
        b"version config"
    );
}

#[cfg(windows)]
#[test]
fn export_rejects_directory_junctions() {
    use std::process::Command;
    let root = temporary();
    let target = target(root.path());
    let outside = temporary();
    fs::create_dir_all(outside.path().join("secret")).unwrap();
    let directory = target.instance.game_directory(root.path()).unwrap();
    fs::create_dir_all(&directory).unwrap();
    let link = directory.join("config");
    assert!(
        Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(outside.path().join("secret"))
            .output()
            .unwrap()
            .status
            .success()
    );
    let output = root.path().join("linked.mrpack");
    let result = export_pack(
        root.path(),
        &target.instance,
        &PackExportOptions::for_instance(&target.instance),
        &output,
        &progress(),
    );
    fs::remove_dir(link).unwrap();
    assert!(result.unwrap_err().contains("联接"));
    assert!(!output.exists());
}
