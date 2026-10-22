use super::*;
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
fn root() -> tempfile::TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&parent).unwrap();
    tempfile::tempdir_in(parent).unwrap()
}
fn downloader() -> Downloader {
    Downloader::new(
        Default::default(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(|_| {}),
    )
    .unwrap()
}
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
struct Server {
    base: String,
    seen: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new(archive: Vec<u8>, bad_hash: bool) -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr());
        let metadata=serde_json::json!([{"binary":{"architecture":match std::env::consts::ARCH {"x86_64"=>"x64","aarch64"=>"aarch64",_=>"x86"},"os":"windows","image_type":"jre","package":{"checksum":if bad_hash {"0".repeat(64)} else {format!("{:x}",Sha256::digest(&archive))},"link":format!("{base}/runtime.zip"),"size":archive.len()}}}]).to_string();
        let seen = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (flag, count) = (stop.clone(), seen.clone());
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                if let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(20)) {
                    let response = if request.url().starts_with("/v3/assets") {
                        tiny_http::Response::from_string(&metadata)
                    } else {
                        count.fetch_add(1, Ordering::Relaxed);
                        tiny_http::Response::from_data(archive.clone())
                    };
                    let _ = request.respond(response);
                }
            }
        });
        Self {
            base,
            seen,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn fake_probe(path: &Path) -> Result<JavaRuntime> {
    if !path.join("bin/java.exe").is_file() {
        return Err("not installed".into());
    }
    Ok(JavaRuntime {
        path: path.join("bin/java.exe"),
        major: 21,
        version: "21-test".into(),
        architecture: std::env::consts::ARCH.into(),
        vendor: "test".into(),
    })
}

#[test]
fn validates_download_extracts_atomically_and_reuses_installed_runtime() {
    let root = root();
    let server = Server::new(
        zip(&[
            ("jre/bin/java.exe", b"fixture"),
            ("jre/lib/modules", b"modules"),
        ]),
        false,
    );
    let d = downloader();
    let runtime = install_at(21, root.path(), &d, &server.base, fake_probe).unwrap();
    assert!(runtime.path.exists());
    assert!(
        runtime
            .path
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("lib/modules")
            .is_file()
    );
    let again = install_at(21, root.path(), &d, &server.base, fake_probe).unwrap();
    assert_eq!(again.path, runtime.path);
    assert_eq!(server.seen.load(Ordering::Relaxed), 1);
}

#[test]
fn bad_hash_cancellation_and_wrong_runtime_never_publish_a_java() {
    let root = root();
    let server = Server::new(zip(&[("jre/bin/java.exe", b"fixture")]), true);
    let d = downloader();
    assert!(
        install_at(21, root.path(), &d, &server.base, fake_probe)
            .unwrap_err()
            .contains("SHA-256")
    );
    assert!(
        !fs::read_dir(root.path())
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("temurin-"))
    );
    d.cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        install_at(21, root.path(), &d, &server.base, fake_probe).unwrap_err(),
        "操作已取消。"
    );
    let server = Server::new(zip(&[("jre/bin/java.exe", b"fixture")]), false);
    let d = downloader();
    assert!(
        install_at(21, root.path(), &d, &server.base, |path| {
            let mut r = fake_probe(path)?;
            r.major = 17;
            Ok(r)
        })
        .unwrap_err()
        .contains("不符")
    );
    assert!(
        !fs::read_dir(root.path())
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("temurin-"))
    );
}

#[test]
fn runtime_zip_rejects_escaping_duplicate_and_mixed_roots() {
    for files in [
        vec![("jre/../../escape", b"x".as_slice())],
        vec![
            ("jre/bin/java.exe", b"x".as_slice()),
            ("other/lib/modules", b"x".as_slice()),
        ],
        vec![
            ("jre/bin/Java.exe", b"x".as_slice()),
            ("jre/bin/java.exe", b"x".as_slice()),
        ],
    ] {
        let root = root();
        let archive = root.path().join("runtime.zip");
        fs::write(&archive, zip(&files)).unwrap();
        assert!(unpack(&archive, &root.path().join("stage"), &downloader()).is_err());
    }
    assert!(!allowed_download("https://evil.example/runtime.zip"));
    assert!(!allowed_download("http://github.com/adoptium/runtime.zip"));
}

#[test]
#[ignore = "downloads and executes an official Temurin Java runtime; run explicitly"]
fn real_temurin_runtime_download_and_probe() {
    let root = root();
    let runtime = install_at(
        21,
        root.path(),
        &downloader(),
        "https://api.adoptium.net",
        probe,
    )
    .unwrap();
    assert!(runtime.compatible(21));
    assert_eq!(probe(&runtime.path).unwrap().major, 21);
    println!("{}", runtime.label());
}
