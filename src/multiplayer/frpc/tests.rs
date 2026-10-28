use super::*;
use crate::{
    accounts::{Account, AccountKind},
    multiplayer::{AccessList, RelayClient, issue_host, issue_member, server::RelayServer},
};
use std::{
    fs,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[test]
fn ticket_is_bound_to_room_challenge_account_and_expiry() {
    let key = crate::multiplayer::secret();
    let challenge = crate::multiplayer::secret();
    let mut ticket = tickets::Ticket::issue("room", &challenge, &"a".repeat(32), &key).unwrap();
    ticket.verify("room", &challenge, &key).unwrap();
    assert!(ticket.verify("other", &challenge, &key).is_err());
    assert!(ticket.verify("room", "other", &key).is_err());
    ticket.account = "b".repeat(32);
    assert!(ticket.verify("room", &challenge, &key).is_err());
}

fn temporary() -> tempfile::TempDir {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&path).unwrap();
    tempfile::tempdir_in(path).unwrap()
}
fn node(port: u16) -> Node {
    Node {
        info: NodeInfo {
            id: "test-node".into(),
            name: "Test node".into(),
            server_addr: "127.0.0.1".into(),
            server_port: port,
            public_host: "127.0.0.1".into(),
        },
        user: "private-fixture-user".into(),
        login_fail_exit: false,
        heartbeat_interval: 10,
        heartbeat_timeout: 60,
        protocol: None,
        proxies: vec![Proxy {
            name: "fixture-proxy".into(),
            remote_port: 12345,
            use_encryption: true,
            use_compression: true,
        }],
    }
}

#[test]
fn imports_concatenated_provider_configs_without_changing_credentials_or_names() {
    let document = |address: &str, user: &str, name: &str, port: u16| {
        format!(
            r#"
serverAddr = "{address}"
serverPort = 1210
user = "{user}"
loginFailExit = false
transport.heartbeatInterval = 10
transport.heartbeatTimeout = 60
[[proxies]]
type = "tcp"
name = "{name}"
localIP = "127.0.0.1"
localPort = 25565
remotePort = {port}
transport.useEncryption = true
transport.useCompression = true
"#
        )
    };
    let text = [
        ("39.106.189.88", "fake-bj", "ProxyBj", 12341),
        ("8.138.226.33", "fake-gz", "ProxyGz", 12342),
        ("116.62.155.225", "fake-hz", "ProxyHz", 12343),
        ("47.100.169.24", "fake-sh", "ProxySh", 12344),
    ]
    .map(|(a, u, n, p)| document(a, u, n, p))
    .join("\n# next node\n");
    let catalog = Catalog::import(&text).unwrap();
    assert_eq!(catalog.nodes.len(), 4);
    let first = &catalog.nodes[0];
    assert_eq!(first.user, "fake-bj");
    assert_eq!(first.info.public_host, "frp-bj.northstarmc.cn");
    let config: serde_json::Value = serde_json::from_slice(
        &Catalog::route(first, &first.proxies[0])
            .config(54321)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(config["user"], "fake-bj");
    assert_eq!(config["serverAddr"], "39.106.189.88");
    assert_eq!(config["serverPort"], 1210);
    assert_eq!(config["loginFailExit"], false);
    assert_eq!(config["proxies"].as_array().unwrap().len(), 1);
    assert_eq!(config["proxies"][0]["name"], "ProxyBj");
    assert_eq!(config["proxies"][0]["remotePort"], 12341);
    assert_eq!(config["proxies"][0]["localPort"], 54321);
    assert_eq!(config["proxies"][0]["transport"]["useEncryption"], true);
    assert!(
        Catalog::import(&(text.clone() + &document("39.106.189.88", "other", "Other", 12400)))
            .is_err()
    );
    assert!(Catalog::import(&text.replace("type = \"tcp\"", "type = \"http\"")).is_err());
}

#[test]
fn randomized_latency_probes_filter_unavailable_nodes_and_support_cancellation() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let live = listener.local_addr().unwrap().port();
    let closed = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let dead = closed.local_addr().unwrap().port();
    drop(closed);
    let mut unavailable = node(dead).info;
    unavailable.id = "dead".into();
    let cancel = Arc::new(AtomicBool::new(false));
    let results = measure_nodes(&[node(live).info, unavailable], &cancel).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].node.server_port, live);
    cancel.store(true, Ordering::Relaxed);
    assert!(
        measure_nodes(&[node(live).info], &cancel)
            .unwrap_err()
            .contains("取消")
    );
}

#[test]
#[ignore = "internal frpc lifecycle fixture; started by process tests"]
fn fake_frpc_process() {
    let Some(path) = std::env::var_os("NSCL_TEST_FRPC_CONFIG") else {
        return;
    };
    let config: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let proxy = &config["proxies"][0];
    let name = proxy["name"].as_str().unwrap();
    match std::env::var("NSCL_TEST_FRPC_MODE").as_deref() {
        Ok("busy") => {
            println!("[{name}] start proxy error: port already used");
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        Ok("waiting") => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        _ => {}
    }
    let listener =
        TcpListener::bind(("127.0.0.1", proxy["remotePort"].as_u64().unwrap() as u16)).unwrap();
    let local = proxy["localPort"].as_u64().unwrap() as u16;
    println!("[{name}] start proxy success");
    std::io::stdout().flush().unwrap();
    for incoming in listener.incoming() {
        let mut front = incoming.unwrap();
        std::thread::spawn(move || {
            let Ok(mut back) = TcpStream::connect(("127.0.0.1", local)) else {
                return;
            };
            let mut a = front.try_clone().unwrap();
            let mut b = back.try_clone().unwrap();
            let up = std::thread::spawn(move || {
                let _ = std::io::copy(&mut a, &mut b);
                let _ = b.shutdown(Shutdown::Write);
            });
            let _ = std::io::copy(&mut back, &mut front);
            let _ = front.shutdown(Shutdown::Write);
            let _ = up.join();
        });
    }
}

fn fixture_command(mode: &str) -> std::process::Command {
    let mut command = crate::minecraft::hidden_command(std::env::current_exe().unwrap());
    command
        .args([
            "--ignored",
            "--exact",
            "multiplayer::frpc::tests::fake_frpc_process",
            "--nocapture",
        ])
        .env("NSCL_TEST_FRPC_MODE", mode);
    command
}
#[test]
fn frpc_process_reports_readiness_busy_cancellation_and_cleans_private_files() {
    let directory = temporary();
    let port = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let remote = port.local_addr().unwrap().port();
    drop(port);
    let mut n = node(1210);
    n.proxies[0].remote_port = remote;
    let route = Catalog::route(&n, &n.proxies[0]);
    let cancel = Arc::new(AtomicBool::new(false));
    let process = super::FrpcProcess::spawn(
        &mut fixture_command("ready"),
        &route,
        25565,
        directory.path(),
        &cancel,
    );
    assert!(process.is_ok(), "frpc fixture failed to start");
    let mut process = process.ok().unwrap();
    assert!(process.running());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    drop(process);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    let busy = super::FrpcProcess::spawn(
        &mut fixture_command("busy"),
        &route,
        25565,
        directory.path(),
        &cancel,
    );
    assert!(matches!(busy, Err(super::process::Failure::Busy)));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    let flag = cancel.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        flag.store(true, Ordering::Relaxed);
    });
    let cancelled = super::FrpcProcess::spawn(
        &mut fixture_command("waiting"),
        &route,
        25565,
        directory.path(),
        &cancel,
    );
    worker.join().unwrap();
    assert!(matches!(cancelled, Err(super::process::Failure::Cancelled)));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

struct Fixture {
    directory: tempfile::TempDir,
    endpoint: String,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    control: TcpListener,
    alice: Account,
    bob: Account,
    alice_token: String,
    bob_token: String,
}
impl Fixture {
    fn new() -> Self {
        let directory = temporary();
        let mut list = AccessList::default();
        let alice_token = issue_member(&mut list, AccountKind::Offline, "", "Alice").unwrap();
        let bob_token = issue_member(&mut list, AccountKind::Offline, "", "Bob").unwrap();
        crate::store::write_json(&directory.path().join("members.json"), &list).unwrap();
        let control = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let remote = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let remote_port = remote.local_addr().unwrap().port();
        drop(remote);
        let mut n = node(control.local_addr().unwrap().port());
        n.proxies[0].remote_port = remote_port;
        let catalog = directory.path().join("nodes.json");
        crate::store::write_json(&catalog, &Catalog { nodes: vec![n] }).unwrap();
        let service = Arc::new(
            RelayServer::new(&directory.path().join("members.json"))
                .unwrap()
                .with_frpc_catalog(&catalog)
                .unwrap(),
        );
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", server.server_addr());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let worker = std::thread::spawn(move || service.serve(server, flag).unwrap());
        Self {
            directory,
            endpoint,
            stop,
            worker: Some(worker),
            control,
            alice: Account::offline("Alice").unwrap(),
            bob: Account::offline("Bob").unwrap(),
            alice_token,
            bob_token,
        }
    }
    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        alice: bool,
        body: Vec<u8>,
    ) -> reqwest::blocking::Response {
        reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
            .request(method, format!("{}/{path}", self.endpoint))
            .bearer_auth(if alice {
                &self.alice_token
            } else {
                &self.bob_token
            })
            .body(body)
            .send()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

#[test]
fn public_service_check_reports_readiness_without_credentials() {
    let fixture = Fixture::new();
    let client = RelayClient::public(&fixture.endpoint).unwrap();
    let info = client.check_service().unwrap();
    assert_eq!(info.service, SERVICE_NAME);
    assert!(info.host_codes);
    assert_eq!(info.game_port, 25565);
    assert_eq!(
        (info.protocol, info.max_hosts, info.nodes, info.tunnels),
        (1, 28, 1, 1)
    );
    let body = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("{}/v1/frpc/health", fixture.endpoint))
        .send()
        .unwrap()
        .text()
        .unwrap();
    assert!(!body.contains("private-fixture-user"));
    assert!(!body.contains("fixture-proxy"));
    fs::remove_file(fixture.directory.path().join("nodes.json")).unwrap();
    assert!(client.check_service().is_err());
}

#[test]
fn public_host_entry_rejects_ports_other_than_25565() {
    let client = RelayClient::new("http://127.0.0.1:1", &"a".repeat(64)).unwrap();
    let result = client.host_frpc_room(
        25566,
        HostOptions {
            executable: std::env::current_exe().unwrap(),
            runtime_directory: Path::new("unused").into(),
            cancel: Arc::new(AtomicBool::new(false)),
            report: Arc::new(|_| {}),
        },
    );
    assert!(result.err().unwrap().contains("25565"));
}

#[test]
fn service_allocates_exclusive_tunnels_hides_credentials_and_frees_leases() {
    let fixture = Fixture::new();
    let body = serde_json::to_vec(&CreateRoom {
        candidates: vec!["test-node".into()],
        avoid: vec![],
    })
    .unwrap();
    let nodes = fixture
        .request(reqwest::Method::GET, "v1/frpc/nodes", true, vec![])
        .text()
        .unwrap();
    assert!(!nodes.contains("private-fixture-user"));
    assert!(!nodes.contains("fixture-proxy"));
    let lease: Lease = fixture
        .request(reqwest::Method::POST, "v1/frpc/rooms", true, body.clone())
        .json()
        .unwrap();
    assert_eq!(
        fixture
            .request(reqwest::Method::POST, "v1/frpc/rooms", false, body.clone())
            .status(),
        409
    );
    let info = fixture
        .request(
            reqwest::Method::GET,
            &format!("v1/frpc/rooms/{}", lease.room),
            false,
            vec![],
        )
        .text()
        .unwrap();
    assert!(!info.contains(&lease.key));
    assert!(!info.contains("private-fixture-user"));
    assert_eq!(
        fixture
            .request(
                reqwest::Method::GET,
                &format!("v1/frpc/rooms/{}/status", lease.room),
                false,
                vec![]
            )
            .status(),
        404
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::DELETE,
                &format!("v1/rooms/{}", lease.room),
                true,
                vec![]
            )
            .status(),
        204
    );
    assert_eq!(
        fixture
            .request(reqwest::Method::POST, "v1/frpc/rooms", false, body)
            .status(),
        200
    );
}

#[test]
fn twenty_eight_host_slots_do_not_limit_public_guests() {
    let fixture = Fixture::new();
    let members = fixture.directory.path().join("members.json");
    let mut list = AccessList::default();
    let tokens: Vec<_> = (0..29).map(|_| issue_host(&mut list).unwrap().1).collect();
    crate::store::write_json(&members, &list).unwrap();
    let mut n = node(fixture.control.local_addr().unwrap().port());
    n.proxies = (0..29)
        .map(|i| Proxy {
            name: format!("fixture-{i}"),
            remote_port: 20000 + i,
            use_encryption: true,
            use_compression: true,
        })
        .collect();
    crate::store::write_json(
        &fixture.directory.path().join("nodes.json"),
        &Catalog { nodes: vec![n] },
    )
    .unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let rooms = format!("{}/v1/frpc/rooms", fixture.endpoint);
    let selection = serde_json::json!({"candidates":["test-node"],"avoid":[]});
    assert_eq!(
        client
            .post(&rooms)
            .json(&selection)
            .send()
            .unwrap()
            .status(),
        401
    );
    let mut leases = Vec::new();
    let mut slots = std::collections::BTreeSet::new();
    for token in &tokens[..28] {
        let lease: Lease = client
            .post(&rooms)
            .bearer_auth(token)
            .json(&selection)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap();
        assert!(slots.insert(lease.route.slot()));
        leases.push(lease);
    }
    assert_eq!(
        client
            .post(&rooms)
            .bearer_auth(&tokens[28])
            .json(&selection)
            .send()
            .unwrap()
            .status(),
        409
    );
    let room = format!("{}/{}", rooms, leases[0].room);
    assert_eq!(
        client
            .get(format!("{room}/status"))
            .send()
            .unwrap()
            .status(),
        401
    );
    let info = client.get(&room).send().unwrap().text().unwrap();
    assert!(!info.contains("private-fixture-user"));
    assert!(!info.contains(&leases[0].key));
    // Beyond the previous six-per-account and 120-total limits, with no credentials.
    for _ in 0..131 {
        let challenge = crate::multiplayer::secret();
        let ticket: tickets::Ticket = client
            .post(format!("{room}/tickets"))
            .json(&TicketRequest {
                challenge: challenge.clone(),
            })
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap();
        ticket
            .verify(&leases[0].room, &challenge, &leases[0].key)
            .unwrap();
    }
    assert_eq!(
        client
            .delete(format!(
                "{}/v1/frpc/rooms/{}",
                fixture.endpoint, leases[0].room
            ))
            .bearer_auth(&tokens[0])
            .send()
            .unwrap()
            .status(),
        204
    );
    assert_eq!(
        client
            .post(&rooms)
            .bearer_auth(&tokens[28])
            .json(&selection)
            .send()
            .unwrap()
            .status(),
        200
    );
}

#[test]
fn managed_frpc_fixture_and_authorized_game_stream_reject_raw_connections_and_revocation() {
    let fixture = Fixture::new();
    let members = fixture.directory.path().join("members.json");
    let mut list = AccessList::load(&members).unwrap();
    let (host_id, host_token) = issue_host(&mut list).unwrap();
    assert_ne!(host_id, fixture.alice.id);
    crate::store::write_json(&members, &list).unwrap();
    let _control_port = fixture.control.local_addr().unwrap();
    let game = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = game.local_addr().unwrap().port();
    game.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let accepted = Arc::new(Mutex::new(0usize));
    let count = accepted.clone();
    let echo = std::thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            match game.accept() {
                Ok((mut stream, _)) => {
                    *count.lock().unwrap() += 1;
                    std::thread::spawn(move || {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(10)))
                            .unwrap();
                        let mut buffer = [0; 8192];
                        while let Ok(n) = stream.read(&mut buffer) {
                            if n == 0 {
                                break;
                            }
                            if stream.write_all(&buffer[..n]).is_err() {
                                break;
                            }
                        }
                    });
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    });
    let cancel = Arc::new(AtomicBool::new(false));
    let report = Arc::new(|_: String| {});
    // The TCP fixture uses an ephemeral game port; the public entry's fixed-port rule is tested above.
    let host = RelayClient::new(&fixture.endpoint, &host_token)
        .unwrap()
        .host_frpc_local(
            port,
            HostOptions {
                executable: std::env::current_exe().unwrap(),
                runtime_directory: fixture.directory.path().join("runtime"),
                cancel: cancel.clone(),
                report,
            },
        )
        .unwrap();
    assert!(host.running());
    assert!(host.room.starts_with("frpc:"));
    // A raw TCP visitor cannot pass the authenticated handshake to reach Minecraft.
    let mut intruder = TcpStream::connect(&host.public_address).unwrap();
    intruder
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let challenge: serde_json::Value = tickets::receive(&mut intruder).unwrap();
    let forged = tickets::Ticket::issue(
        challenge["room"].as_str().unwrap(),
        challenge["challenge"].as_str().unwrap(),
        &"a".repeat(32),
        "wrong-key",
    )
    .unwrap();
    tickets::send(&mut intruder, &forged).unwrap();
    let mut byte = [0];
    assert!(intruder.read(&mut byte).is_err() || byte[0] == 0);
    let guest = RelayClient::public(&fixture.endpoint)
        .unwrap()
        .join_frpc_room(&host.room, Arc::new(AtomicBool::new(false)))
        .unwrap();
    let mut stream = TcpStream::connect(guest.local_address.unwrap()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let bytes: Vec<_> = (0..256 * 1024).map(|i| i as u8).collect();
    stream.write_all(&bytes).unwrap();
    let mut reply = vec![0; bytes.len()];
    stream.read_exact(&mut reply).unwrap();
    assert_eq!(reply, bytes);
    let mut list = AccessList::load(&members).unwrap();
    list.members.retain(|m| m.identity.id != fixture.bob.id);
    crate::store::write_json(&members, &list).unwrap();
    std::thread::sleep(Duration::from_millis(2200));
    assert!(guest.running());
    stream.write_all(b"still allowed").unwrap();
    let mut reply = [0; 13];
    stream.read_exact(&mut reply).unwrap();
    assert_eq!(&reply, b"still allowed");
    list.host_keys.retain(|key| key.id != host_id);
    crate::store::write_json(&members, &list).unwrap();
    let deadline = Instant::now() + Duration::from_secs(7);
    while guest.running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!guest.running());
    while host.connections() > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(host.connections(), 0);
    host.stop();
    let deadline = Instant::now() + Duration::from_secs(7);
    while fs::read_dir(fixture.directory.path().join("runtime"))
        .unwrap()
        .count()
        > 0
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        fs::read_dir(fixture.directory.path().join("runtime"))
            .unwrap()
            .count(),
        0
    );
    stop.store(true, Ordering::Relaxed);
    echo.join().unwrap();
}
