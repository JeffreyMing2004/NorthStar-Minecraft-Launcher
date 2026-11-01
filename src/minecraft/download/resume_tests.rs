use super::*;
use std::{
    collections::VecDeque,
    io::BufRead,
    net::{TcpListener, TcpStream},
    thread,
};

struct Server {
    url: String,
    seen: Arc<Mutex<Vec<String>>>,
    done: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(replies: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let (requests, stop) = (seen.clone(), done.clone());
        let worker = thread::spawn(move || {
            let mut replies = VecDeque::from(replies);
            while !stop.load(Ordering::Relaxed) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut input = std::io::BufReader::new(stream.try_clone().unwrap());
                    let mut request = String::new();
                    loop {
                        let mut line = String::new();
                        if input.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                        request.push_str(&line);
                    }
                    requests.lock().unwrap().push(request.to_ascii_lowercase());
                    if let Some(reply) = replies.pop_front() {
                        let _ = stream.write_all(reply.as_bytes());
                    }
                } else {
                    thread::sleep(Duration::from_millis(5));
                }
            }
        });
        Self {
            url,
            seen,
            done,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(
            self.url
                .trim_start_matches("http://")
                .trim_end_matches("/file"),
        );
        self.worker.take().unwrap().join().unwrap();
    }
}
fn reply(status: &str, headers: &str, length: usize, data: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n{headers}\r\n{data}"
    )
}
fn job(root: &Path, url: &str) -> Job {
    Job {
        url: url.into(),
        path: root.join("payload.bin"),
        sha1: format!("{:x}", Sha1::digest(b"abcdefgh")),
        size: Some(8),
    }
}
fn downloader() -> Downloader {
    Downloader::new(
        Source {
            proxy_mode: ProxyMode::Direct,
            ..Default::default()
        },
        Arc::new(AtomicBool::new(false)),
        Arc::new(|_| {}),
    )
    .unwrap()
}
fn temporary() -> tempfile::TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&parent).unwrap();
    tempfile::tempdir_in(parent).unwrap()
}
fn seed(job: &Job) {
    fs::write(job.path.with_extension("bin.part"), b"abc").unwrap();
    write_json(
        &job.path.with_extension("bin.part.json"),
        &Resume {
            url: job.url.clone(),
            sha1: job.sha1.clone(),
            size: job.size,
            validator: Some("\"v1\"".into()),
        },
    )
    .unwrap();
}
#[test]
fn interrupted_transfer_resumes_across_downloader_instances_with_range_and_validator() {
    let root = temporary();
    let server = Server::new(vec![
        reply("200 OK", "ETag: \"v1\"\r\n", 8, "abc"),
        reply(
            "206 Partial Content",
            "Content-Range: bytes 3-7/8\r\nETag: \"v1\"\r\n",
            5,
            "de",
        ),
        reply(
            "206 Partial Content",
            "Content-Range: bytes 5-7/8\r\nETag: \"v1\"\r\n",
            3,
            "fgh",
        ),
    ]);
    let job = job(root.path(), &server.url);
    fs::write(&job.path, b"previous destination").unwrap();
    assert!(
        downloader()
            .resumable(&job, std::slice::from_ref(&server.url))
            .is_err()
    );
    assert_eq!(fs::read(&job.path).unwrap(), b"previous destination");
    assert_eq!(
        fs::read(job.path.with_extension("bin.part")).unwrap(),
        b"abcde"
    );
    downloader()
        .resumable(&job, std::slice::from_ref(&server.url))
        .unwrap();
    assert_eq!(fs::read(&job.path).unwrap(), b"abcdefgh");
    let seen = server.seen.lock().unwrap();
    assert!(seen[1].contains("range: bytes=3-"));
    assert!(seen[2].contains("range: bytes=5-"));
    assert!(seen[2].contains("if-range: \"v1\""));
    assert!(!job.path.with_extension("bin.part.json").exists());
}
#[test]
fn ignored_or_invalid_ranges_restart_without_appending_old_bytes() {
    for invalid in [false, true] {
        let root = temporary();
        let full = reply("200 OK", "ETag: \"v2\"\r\n", 8, "abcdefgh");
        let replies = if invalid {
            vec![
                reply(
                    "206 Partial Content",
                    "Content-Range: bytes 0-7/8\r\n",
                    8,
                    "abcdefgh",
                ),
                full,
            ]
        } else {
            vec![full]
        };
        let server = Server::new(replies);
        let job = job(root.path(), &server.url);
        seed(&job);
        downloader()
            .resumable(&job, std::slice::from_ref(&server.url))
            .unwrap();
        assert_eq!(fs::read(&job.path).unwrap(), b"abcdefgh");
        let seen = server.seen.lock().unwrap();
        assert!(seen[0].contains("range: bytes=3-"), "{seen:?}");
        if invalid {
            assert!(!seen[1].contains("range:"));
        }
    }
}
#[test]
fn bad_hash_never_replaces_destination_and_paused_downloads_remain_cancellable() {
    let root = temporary();
    let bad = reply("200 OK", "", 8, "badbytes");
    let server = Server::new(vec![bad.clone(), bad]);
    let job = job(root.path(), &server.url);
    fs::write(&job.path, b"original").unwrap();
    let d = downloader();
    assert!(
        d.resumable(&job, std::slice::from_ref(&server.url))
            .unwrap_err()
            .contains("SHA-1")
    );
    assert_eq!(fs::read(&job.path).unwrap(), b"original");
    assert!(!job.path.with_extension("bin.part").exists());
    d.source.paused.store(true, Ordering::Relaxed);
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = d.clone();
    let thread = thread::spawn(move || {
        tx.send(worker.check_cancelled()).unwrap();
    });
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    d.cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap_err(),
        "操作已取消。"
    );
    thread.join().unwrap();
}
