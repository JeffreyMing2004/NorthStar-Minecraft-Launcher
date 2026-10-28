use super::*;
use crate::accounts::{Account, AccountKind};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn temporary() -> tempfile::TempDir {
    let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    std::fs::create_dir_all(&parent).unwrap();
    tempfile::tempdir_in(parent).unwrap()
}

struct Fixture {
    directory: tempfile::TempDir,
    endpoint: String,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    alice: Account,
    bob: Account,
    alice_token: String,
    bob_token: String,
}
impl Fixture {
    fn new() -> Self {
        let directory = temporary();
        let mut list = AccessList::default();
        let alice = Account::offline("Alice").unwrap();
        let bob = Account::offline("Bob").unwrap();
        let alice_token = issue_member(&mut list, AccountKind::Offline, "", "Alice").unwrap();
        let bob_token = issue_member(&mut list, AccountKind::Offline, "", "Bob").unwrap();
        let path = directory.path().join("members.json");
        crate::store::write_json(&path, &list).unwrap();
        let relay = Arc::new(server::RelayServer::new(&path).unwrap());
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", server.server_addr());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let worker = std::thread::spawn(move || relay.serve(server, flag).unwrap());
        Self {
            directory,
            endpoint,
            stop,
            worker: Some(worker),
            alice,
            bob,
            alice_token,
            bob_token,
        }
    }
    fn alice_client(&self) -> RelayClient {
        RelayClient::new(&self.endpoint, &self.alice_token).unwrap()
    }
    fn bob_client(&self) -> RelayClient {
        RelayClient::new(&self.endpoint, &self.bob_token).unwrap()
    }
    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        token: &str,
        bytes: &[u8],
    ) -> reqwest::blocking::Response {
        reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
            .request(method, format!("{}/{path}", self.endpoint))
            .bearer_auth(token)
            .body(bytes.to_vec())
            .send()
            .unwrap()
    }
    fn room(&self) -> String {
        self.request(reqwest::Method::POST, "v1/rooms", &self.alice_token, &[])
            .json::<serde_json::Value>()
            .unwrap()["room"]
            .as_str()
            .unwrap()
            .into()
    }
    fn connection(&self, room: &str) -> String {
        self.request(
            reqwest::Method::POST,
            &format!("v1/rooms/{room}/join"),
            &self.bob_token,
            &[],
        )
        .json::<serde_json::Value>()
        .unwrap()["connection"]
            .as_str()
            .unwrap()
            .into()
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
fn independent_host_codes_need_no_player_enrollment_and_store_only_hashes() {
    let mut list = AccessList::default();
    let (id, token) = issue_host(&mut list).unwrap();
    assert!(list.members.is_empty());
    assert_eq!(list.host_keys.len(), 1);
    let owner = list.authenticate_host(&token).unwrap();
    assert_eq!(owner.id, id);
    assert!(owner.account.is_none());
    assert!(list.host_active(&owner));
    assert!(list.authenticate(&token).is_none());
    assert!(list.authenticate_host(&"0".repeat(64)).is_none());
    let json = serde_json::to_string(&list).unwrap();
    assert!(!json.contains(&token));
    assert!(!json.contains("name"));
    assert!(!json.contains("kind"));
    let mut duplicate = list.clone();
    duplicate.host_keys.push(list.host_keys[0].clone());
    assert!(duplicate.validate().is_err());
    list.host_keys.clear();
    assert!(!list.host_active(&owner));
    assert!(list.authenticate_host(&token).is_none());
    assert!(
        serde_json::from_str::<AccessList>("{\"members\":[]}")
            .unwrap()
            .host_keys
            .is_empty()
    );
}

#[test]
fn host_registry_preserves_unique_identities_without_a_player_quota() {
    let mut list = AccessList::default();
    for i in 0..32 {
        issue_member(&mut list, AccountKind::Offline, "", &format!("Player{i}")).unwrap();
    }
    issue_member(&mut list, AccountKind::Offline, "", "Extra").unwrap();
    assert_eq!(list.members.len(), 33);
    assert!(issue_member(&mut list, AccountKind::Offline, "", "Player0").is_err());
    let serialized = serde_json::to_string(&list).unwrap();
    assert!(!serialized.contains("Bearer"));
    let mut invalid = list.clone();
    invalid.members[0].identity.id = "0".repeat(32);
    assert!(invalid.validate().is_err());
    let mut invalid = list.clone();
    invalid.members[1].token_sha256 = invalid.members[0].token_sha256.clone();
    assert!(invalid.validate().is_err());
    list.members[0].identity.kind = AccountKind::Microsoft;
    list.validate().unwrap();
}

#[test]
fn credentials_are_required_for_both_kinds_and_bound_to_the_selected_account() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.alice_client().authorize(&fixture.alice).unwrap().id,
        fixture.alice.id
    );
    assert!(
        fixture
            .alice_client()
            .authorize(&fixture.bob)
            .unwrap_err()
            .contains("不属于")
    );
    let token = "0".repeat(64);
    assert_eq!(
        fixture
            .request(reqwest::Method::POST, "v1/rooms", &token, &[])
            .status(),
        401
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::POST,
                "v1/enroll",
                &fixture.alice_token,
                &[]
            )
            .status(),
        404
    );
    let path = fixture.directory.path().join("members.json");
    let mut list = AccessList::load(&path).unwrap();
    let ms_token = issue_member(
        &mut list,
        AccountKind::Microsoft,
        "12345678-1234-1234-1234-123456789abc",
        "OnlinePlayer",
    )
    .unwrap();
    crate::store::write_json(&path, &list).unwrap();
    let mut account = Account::offline("OnlinePlayer").unwrap();
    account.kind = AccountKind::Microsoft;
    account.id = "12345678123412341234123456789abc".into();
    assert_eq!(
        RelayClient::new(&fixture.endpoint, &ms_token)
            .unwrap()
            .authorize(&account)
            .unwrap()
            .kind,
        AccountKind::Microsoft
    );
    let error = RelayClient::new(&fixture.endpoint, &token)
        .unwrap()
        .authorize(&account)
        .unwrap_err();
    assert!(!error.contains(&token));
}

#[test]
fn public_clients_require_https_and_do_not_accept_credentials_in_urls() {
    let token = "a".repeat(64);
    for endpoint in [
        "http://example.com",
        "ftp://example.com",
        "https://name:secret@example.com",
        "https://example.com?token=secret",
        "https://example.com#token",
    ] {
        assert!(RelayClient::new(endpoint, &token).is_err());
    }
    assert!(RelayClient::new("https://example.com:12345", &token).is_ok());
    assert!(RelayClient::new("http://127.0.0.1:12345", &token).is_ok());
    assert!(RelayClient::new("https://example.com", "bad").is_err());
}

#[test]
fn relay_preserves_binary_frames_acknowledgements_order_and_room_permissions() {
    let fixture = Fixture::new();
    let room = fixture.room();
    let connection = fixture.connection(&room);
    let path = format!("v1/connections/{connection}");
    assert_eq!(
        fixture
            .request(
                reqwest::Method::DELETE,
                &format!("v1/rooms/{room}"),
                &fixture.bob_token,
                &[]
            )
            .status(),
        404
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::POST,
                &format!("{path}/host/send?seq=0"),
                &fixture.bob_token,
                b"attack"
            )
            .status(),
        403
    );
    let bytes: Vec<_> = (0..CHUNK_SIZE).map(|i| i as u8).collect();
    for _ in 0..2 {
        assert_eq!(
            fixture
                .request(
                    reqwest::Method::POST,
                    &format!("{path}/guest/send?seq=0"),
                    &fixture.bob_token,
                    &bytes
                )
                .status(),
            204
        );
    }
    for _ in 0..2 {
        let response = fixture.request(
            reqwest::Method::GET,
            &format!("{path}/host/receive"),
            &fixture.alice_token,
            &[],
        );
        assert_eq!(response.headers()["X-NSCL-Sequence"], "0");
        assert_eq!(response.bytes().unwrap().as_ref(), &bytes);
    }
    assert_eq!(
        fixture
            .request(
                reqwest::Method::GET,
                &format!("{path}/host/receive?ack=8"),
                &fixture.alice_token,
                &[]
            )
            .status(),
        400
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::GET,
                &format!("{path}/host/receive?ack=0"),
                &fixture.alice_token,
                &[]
            )
            .status(),
        204
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::POST,
                &format!("{path}/guest/send?seq=2"),
                &fixture.bob_token,
                b"bad order"
            )
            .status(),
        400
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::POST,
                &format!("{path}/guest/send?seq=1"),
                &fixture.bob_token,
                b"next"
            )
            .status(),
        204
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::GET,
                &format!("{path}/host/receive?ack=0"),
                &fixture.alice_token,
                &[]
            )
            .bytes()
            .unwrap()
            .as_ref(),
        b"next"
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::POST,
                &format!("{path}/host/send?seq=0"),
                &fixture.alice_token,
                b"return"
            )
            .status(),
        204
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::GET,
                &format!("{path}/guest/receive"),
                &fixture.bob_token,
                &[]
            )
            .bytes()
            .unwrap()
            .as_ref(),
        b"return"
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::POST,
                &format!("{path}/host/finish"),
                &fixture.alice_token,
                &[]
            )
            .status(),
        204
    );
    // EOF cannot overtake a frame that the receiver has not acknowledged yet.
    assert_eq!(
        fixture
            .request(
                reqwest::Method::GET,
                &format!("{path}/guest/receive"),
                &fixture.bob_token,
                &[]
            )
            .bytes()
            .unwrap()
            .as_ref(),
        b"return"
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::GET,
                &format!("{path}/guest/receive?ack=0"),
                &fixture.bob_token,
                &[]
            )
            .status(),
        205
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::DELETE,
                &format!("v1/rooms/{room}"),
                &fixture.alice_token,
                &[]
            )
            .status(),
        204
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::GET,
                &format!("{path}/guest/receive"),
                &fixture.bob_token,
                &[]
            )
            .status(),
        410
    );
}

#[test]
fn revoked_members_are_rejected_and_existing_rooms_are_closed() {
    let fixture = Fixture::new();
    let room = fixture.room();
    let connection = fixture.connection(&room);
    let path = fixture.directory.path().join("members.json");
    let mut list = AccessList::load(&path).unwrap();
    list.members.retain(|m| m.identity.id != fixture.alice.id);
    crate::store::write_json(&path, &list).unwrap();
    assert_eq!(
        fixture
            .request(reqwest::Method::GET, "v1/me", &fixture.alice_token, &[])
            .status(),
        401
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::POST,
                &format!("v1/rooms/{room}/join"),
                &fixture.bob_token,
                &[]
            )
            .status(),
        404
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::GET,
                &format!("v1/connections/{connection}/guest/receive"),
                &fixture.bob_token,
                &[]
            )
            .status(),
        410
    );
    std::fs::write(path, b"broken").unwrap();
    assert_eq!(
        fixture
            .request(reqwest::Method::GET, "v1/me", &fixture.bob_token, &[])
            .status(),
        503
    );
}

#[test]
fn queues_and_per_account_connections_are_bounded() {
    let fixture = Fixture::new();
    let room = fixture.room();
    let connection = fixture.connection(&room);
    let bytes = vec![5; CHUNK_SIZE];
    for seq in 0..8 {
        assert_eq!(
            fixture
                .request(
                    reqwest::Method::POST,
                    &format!("v1/connections/{connection}/guest/send?seq={seq}"),
                    &fixture.bob_token,
                    &bytes
                )
                .status(),
            204
        );
    }
    assert_eq!(
        fixture
            .request(
                reqwest::Method::POST,
                &format!("v1/connections/{connection}/guest/send?seq=8"),
                &fixture.bob_token,
                &bytes
            )
            .status(),
        429
    );
    for _ in 0..5 {
        fixture.connection(&room);
    }
    assert_eq!(
        fixture
            .request(
                reqwest::Method::POST,
                &format!("v1/rooms/{room}/join"),
                &fixture.bob_token,
                &[]
            )
            .status(),
        429
    );
    assert_eq!(
        fixture
            .request(
                reqwest::Method::POST,
                &format!("v1/connections/{connection}/host/send?seq=0"),
                &fixture.alice_token,
                &vec![0; CHUNK_SIZE + 1]
            )
            .status(),
        413
    );
}

#[test]
fn hosted_world_and_guest_local_listener_relay_real_tcp_data_end_to_end() {
    let fixture = Fixture::new();
    let echo = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = echo.local_addr().unwrap().port();
    echo.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let echo_stop = stop.clone();
    let echo_thread = std::thread::spawn(move || {
        while !echo_stop.load(Ordering::Relaxed) {
            match echo.accept() {
                Ok((mut stream, _)) => {
                    std::thread::spawn(move || {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        let mut buffer = [0; 8192];
                        let mut first = true;
                        while let Ok(n) = stream.read(&mut buffer) {
                            if n == 0 {
                                break;
                            }
                            if first && buffer[0] == 255 {
                                let _ = stream.write_all(&vec![18; 256 * 1024]);
                                break;
                            }
                            first = false;
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
    let host = fixture.alice_client().host(&fixture.alice, port).unwrap();
    let guest = fixture.bob_client().join(&fixture.bob, &host.room).unwrap();
    for index in 0..2 {
        let mut stream = TcpStream::connect(guest.local_address.unwrap()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        let bytes: Vec<_> = (0..256 * 1024).map(|i| (i + index) as u8).collect();
        stream.write_all(&bytes).unwrap();
        let mut reply = vec![0; bytes.len()];
        let result = stream.read_exact(&mut reply);
        assert!(
            result.is_ok(),
            "round {index}: {result:?}; host={}, guest={}",
            host.message(),
            guest.message()
        );
        assert_eq!(reply, bytes);
        stream.shutdown(Shutdown::Both).unwrap();
    }
    let mut closing = TcpStream::connect(guest.local_address.unwrap()).unwrap();
    closing
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    closing.write_all(&[255]).unwrap();
    let mut final_reply = Vec::new();
    closing.read_to_end(&mut final_reply).unwrap();
    assert_eq!(final_reply, vec![18; 256 * 1024]);
    let address = guest.local_address.unwrap();
    guest.stop();
    host.stop();
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(address).is_ok() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(TcpStream::connect(address).is_err());
    stop.store(true, Ordering::Relaxed);
    echo_thread.join().unwrap();
}
