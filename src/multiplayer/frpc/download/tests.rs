use super::*;
use std::{io::Cursor, thread};

fn temporary() -> tempfile::TempDir {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&path).unwrap();
    tempfile::tempdir_in(path).unwrap()
}
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn archive(payload: &[u8]) -> Vec<u8> {
    if cfg!(windows) {
        return zip(&[
            ("frp/frpc.exe", payload),
            ("frp/frps.exe", b"not the client"),
        ]);
    }
    let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut writer = tar::Builder::new(gzip);
    let mut header = tar::Header::new_gnu();
    header.set_size(payload.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    writer
        .append_data(&mut header, "frp/frpc", payload)
        .unwrap();
    writer.into_inner().unwrap().finish().unwrap()
}
struct Fixture {
    url: String,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new(archive: Vec<u8>, wrong_hash: bool, fallback: bool) -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr());
        let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
        let name = format!(
            "frp_1.2.3_{}.{extension}",
            target(std::env::consts::OS, std::env::consts::ARCH).unwrap()
        );
        let digest = if wrong_hash {
            "0".repeat(64)
        } else {
            format!("{:x}", Sha256::digest(&archive))
        };
        let mut asset = serde_json::json!({"name":name,"size":archive.len(),"browser_download_url":format!("{url}/archive")});
        if !fallback {
            asset["digest"] = format!("sha256:{digest}").into();
        }
        let release=serde_json::to_vec(&serde_json::json!({"tag_name":"v1.2.3","draft":false,"prerelease":false,"assets":[asset,{"name":"frp_sha256_checksums.txt","size":100,"browser_download_url":format!("{url}/sums")}]})).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let worker = thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                if let Some(request) = server.recv_timeout(Duration::from_millis(50)).unwrap() {
                    let bytes = match request.url() {
                        "/latest" => release.clone(),
                        "/archive" => archive.clone(),
                        "/sums" => format!("{digest}  {name}\n").into_bytes(),
                        _ => vec![],
                    };
                    request
                        .respond(tiny_http::Response::from_data(bytes))
                        .unwrap();
                }
            }
        });
        Self {
            url: format!("{url}/latest"),
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

#[test]
fn downloads_verified_client_reuses_cache_and_repairs_tampering() {
    for fallback in [false, true] {
        let root = temporary();
        let payload = b"verified frpc fixture";
        let fixture = Fixture::new(archive(payload), false, fallback);
        let cancel = Arc::new(AtomicBool::new(false));
        let path = install_from(root.path(), &cancel, &|_| {}, &fixture.url).unwrap();
        assert_eq!(fs::read(&path).unwrap(), payload);
        assert_eq!(installed(root.path()), Some(path.clone()));
        assert_eq!(
            install_from(
                root.path(),
                &cancel,
                &|_| {},
                "http://127.0.0.1:1/unavailable"
            )
            .unwrap(),
            path
        );
        fs::write(&path, b"tampered").unwrap();
        assert!(installed(root.path()).is_none());
        let repaired = install_from(root.path(), &cancel, &|_| {}, &fixture.url).unwrap();
        assert_eq!(fs::read(repaired).unwrap(), payload);
        assert!(!fs::read_dir(root.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".frpc-")
        }));
    }
}

#[test]
fn checksum_failure_and_cancellation_leave_no_install_or_partial_files() {
    let root = temporary();
    let fixture = Fixture::new(archive(b"fixture"), true, false);
    let cancel = Arc::new(AtomicBool::new(false));
    assert!(
        install_from(root.path(), &cancel, &|_| {}, &fixture.url)
            .unwrap_err()
            .contains("SHA-256")
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    cancel.store(true, Ordering::Relaxed);
    assert!(
        install_from(root.path(), &cancel, &|_| {}, &fixture.url)
            .unwrap_err()
            .contains("取消")
    );
    cancel.store(false, Ordering::Relaxed);
    let fixture = Fixture::new(archive(b"fixture"), false, false);
    let flag = cancel.clone();
    let report = move |message: String| {
        if message.contains("正在下载 frpc") {
            flag.store(true, Ordering::Relaxed);
        }
    };
    assert!(
        install_from(root.path(), &cancel, &report, &fixture.url)
            .unwrap_err()
            .contains("取消")
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn extraction_rejects_traversal_duplicates_and_missing_client() {
    let root = temporary();
    let cancel = Arc::new(AtomicBool::new(false));
    let name = if cfg!(windows) { "frpc.exe" } else { "frpc" };
    for (i, entries) in [
        vec![(format!("../{name}"), b"escape".as_slice())],
        vec![
            (format!("a/{name}"), b"a".as_slice()),
            (format!("b/{name}"), b"b".as_slice()),
        ],
        vec![("frps".into(), b"server".as_slice())],
    ]
    .into_iter()
    .enumerate()
    {
        let archive = root.path().join(format!("{i}.zip"));
        let destination = root.path().join(format!("{i}.exe"));
        fs::write(
            &archive,
            zip(&entries
                .iter()
                .map(|(p, b)| (p.as_str(), *b))
                .collect::<Vec<_>>()),
        )
        .unwrap();
        assert!(extract(&archive, &destination, "zip", &cancel).is_err());
    }
    assert!(!root.path().parent().unwrap().join(name).exists());
}
#[test]
fn official_artifacts_are_platform_specific_and_require_valid_hashes() {
    assert_eq!(target("windows", "x86_64").unwrap(), "windows_amd64");
    assert_eq!(target("macos", "aarch64").unwrap(), "darwin_arm64");
    assert!(validate_url("https://example.com/frpc.exe").is_err());
    assert!(validate_url("https://github.com/other/repo/releases/download/v1/frpc.exe").is_err());
    assert!(validate_url("https://github.com/fatedier/frp/releases/download/v1/frpc.zip").is_ok());
    assert!(valid_hash("bad").is_err());
    assert_eq!(
        checksum(&format!("{}  archive.zip\n", "a".repeat(64)), "archive.zip").unwrap(),
        "a".repeat(64)
    );
}
