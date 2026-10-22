use super::*;
use std::io::{Cursor, Write};

fn temporary() -> tempfile::TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&parent).unwrap();
    tempfile::tempdir_in(parent).unwrap()
}
fn release() -> Release {
    serde_json::from_value(serde_json::json!({"tag_name":"v1.10.0","html_url":format!("{RELEASES_URL}/tag/v1.10.0"),"draft":false,"prerelease":false,"assets":[
        {"name":"nscl-v1.10.0-windows-x86_64.zip","size":200,"browser_download_url":format!("https://github.com/{REPOSITORY}/releases/download/v1.10.0/nscl-v1.10.0-windows-x86_64.zip"),"digest":format!("sha256:{}", "a".repeat(64))},
        {"name":"nscl-v1.10.0-windows-aarch64.zip","size":200,"browser_download_url":"https://example.invalid/wrong"}
    ]})).unwrap()
}
fn executable() -> Vec<u8> {
    let mut bytes = vec![0; 256];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&128u32.to_le_bytes());
    bytes[128..132].copy_from_slice(b"PE\0\0");
    let machine: u16 = if cfg!(target_arch = "aarch64") {
        0xaa64
    } else {
        0x8664
    };
    bytes[132..134].copy_from_slice(&machine.to_le_bytes());
    bytes
}
#[test]
fn release_versions_assets_and_download_provenance_are_checked() {
    let mut r = release();
    assert!(r.newer_than("1.9.0").unwrap());
    assert!(!r.newer_than("1.10.0").unwrap());
    assert!(!r.newer_than("2.0.0").unwrap());
    if cfg!(windows) {
        assert!(r.asset().unwrap().name.contains(std::env::consts::ARCH));
    }
    validate_release(&r).unwrap();
    validate_asset(&r.assets[0]).unwrap();
    r.assets[0].browser_download_url = "https://example.com/nscl.exe".into();
    assert!(validate_asset(&r.assets[0]).is_err());
    r.prerelease = true;
    assert!(validate_release(&r).is_err());
    r.prerelease = false;
    r.tag_name = "v3.0.0-rc.1".into();
    assert!(!r.newer_than("1.0.0").unwrap());
    assert!(parse_hash("sha256:abc").is_err());
    assert_eq!(
        parse_hash(&format!("sha256:{}", "A".repeat(64))).unwrap(),
        "a".repeat(64)
    );
}
#[test]
fn zip_extracts_only_the_single_launcher_and_rejects_bad_payloads() {
    let dir = temporary();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("folder/NSCL.exe", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&executable()).unwrap();
    zip.start_file(
        "../../do-not-extract.txt",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"ignored").unwrap();
    let archive = dir.path().join("update.zip");
    fs::write(&archive, zip.finish().unwrap().into_inner()).unwrap();
    let payload = dir.path().join("payload.exe");
    extract_payload(&archive, &payload).unwrap();
    assert_eq!(fs::read(&payload).unwrap(), executable());
    fs::write(&payload, b"not a PE").unwrap();
    assert!(validate_executable(&payload).is_err());
}
#[test]
fn replacement_preserves_a_backup_and_rejects_changed_payloads() {
    let dir = temporary();
    let target = dir.path().join("NSCL.exe");
    let payload = dir.path().join("payload.exe");
    fs::write(&target, b"original launcher").unwrap();
    fs::write(&payload, executable()).unwrap();
    let plan = ApplyPlan {
        parent_pid: 0,
        old_hash: file_hash(&target).unwrap(),
        new_hash: file_hash(&payload).unwrap(),
        target: target.clone(),
        payload: payload.clone(),
    };
    fs::write(&payload, b"tampered").unwrap();
    assert!(replace_payload(&plan).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"original launcher");
    fs::write(&payload, executable()).unwrap();
    let backup = replace_payload(&plan).unwrap();
    assert_eq!(fs::read(backup).unwrap(), b"original launcher");
    assert_eq!(fs::read(target).unwrap(), executable());
}

#[test]
fn cancelled_download_and_modified_ready_payload_are_rejected_before_network_or_restart() {
    let error = download(&release(), &AtomicBool::new(true), |_, _| {}).unwrap_err();
    assert!(error.contains("已取消"));
    if cfg!(windows) {
        let dir = temporary();
        let path = dir.path().join("payload.exe");
        fs::write(&path, executable()).unwrap();
        let update = ReadyUpdate {
            executable: path.clone(),
            sha256: file_hash(&path).unwrap(),
        };
        fs::write(&path, b"changed").unwrap();
        assert!(
            restart_with_update(&update)
                .unwrap_err()
                .contains("发生变化")
        );
        assert!(!dir.path().join("update-helper.exe").exists());
    }
}

#[test]
#[ignore = "requires access to the public GitHub Releases API"]
fn live_github_release_check() {
    let release = check().unwrap();
    eprintln!(
        "GitHub latest: {}",
        release
            .map(|r| r.tag_name)
            .unwrap_or_else(|| "no published release".into())
    );
}

#[test]
fn download_stream_requires_exact_size_hash_and_respects_cancellation() {
    let bytes = vec![37u8; 200_000];
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let cancelled = AtomicBool::new(false);
    let mut output = Vec::new();
    receive_asset(
        Cursor::new(&bytes),
        &mut output,
        bytes.len() as u64,
        &hash,
        &cancelled,
        &|_, _| {},
    )
    .unwrap();
    assert_eq!(output, bytes);
    for (size, digest) in [
        (199_999, hash.clone()),
        (200_001, hash.clone()),
        (200_000, "0".repeat(64)),
    ] {
        assert!(
            receive_asset(
                Cursor::new(&bytes),
                &mut Vec::new(),
                size,
                &digest,
                &cancelled,
                &|_, _| {}
            )
            .is_err()
        );
    }
    let mut partial = Vec::new();
    let error = receive_asset(
        Cursor::new(&bytes),
        &mut partial,
        bytes.len() as u64,
        &hash,
        &cancelled,
        &|_, _| {
            cancelled.store(true, Ordering::Relaxed);
        },
    )
    .unwrap_err();
    assert!(error.contains("已取消"));
    assert!(partial.len() < bytes.len());
}
