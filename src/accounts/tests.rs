use super::*;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    thread,
};

const PROFILE: &str = "0123456789abcdef0123456789abcdef";
type Replies = BTreeMap<String, VecDeque<(u16, Value)>>;
pub(super) struct Server {
    pub(super) client: AuthClient,
    routes: Arc<Mutex<Replies>>,
    pub(super) requests: Arc<Mutex<Vec<(String, String)>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    pub(super) fn new() -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr().to_ip().unwrap());
        let routes = Arc::new(Mutex::new(Replies::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, q, s) = (routes.clone(), requests.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                if let Ok(Some(mut request)) = server.recv_timeout(Duration::from_millis(50)) {
                    let mut body = String::new();
                    request.as_reader().read_to_string(&mut body).unwrap();
                    let path = request.url().split('?').next().unwrap().to_owned();
                    q.lock().unwrap().push((path.clone(), body));
                    let (status, value) = r
                        .lock()
                        .unwrap()
                        .get_mut(&path)
                        .and_then(VecDeque::pop_front)
                        .unwrap_or((404, json!({})));
                    let response = tiny_http::Response::from_string(value.to_string())
                        .with_status_code(status)
                        .with_header(
                            tiny_http::Header::from_bytes("Content-Type", "application/json")
                                .unwrap(),
                        );
                    let _ = request.respond(response);
                }
            }
        });
        let client = AuthClient {
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            endpoints: Endpoints {
                microsoft: format!("{base}/ms"),
                xbox: format!("{base}/xbox"),
                xsts: format!("{base}/xsts"),
                minecraft: format!("{base}/mc"),
            },
        };
        Self {
            client,
            routes,
            requests,
            stop,
            worker: Some(worker),
        }
    }
    pub(super) fn put(&self, path: &str, status: u16, value: Value) {
        self.routes
            .lock()
            .unwrap()
            .entry(path.into())
            .or_default()
            .push_back((status, value));
    }
    fn device(&self) {
        self.put("/ms/devicecode", 200, json!({"device_code":"private-device-code", "user_code":"USER-CODE", "verification_uri":"https://www.microsoft.com/link", "expires_in":30, "interval":1}));
    }
    pub(super) fn exchange(&self, owns: bool) {
        self.put(
            "/xbox",
            200,
            json!({"Token":"xbox-token", "DisplayClaims":{"xui":[{"uhs":"hash"}]}}),
        );
        self.put(
            "/xsts",
            200,
            json!({"Token":"xsts-token", "DisplayClaims":{"xui":[{"uhs":"hash","xid":"12345"}]}}),
        );
        self.put(
            "/mc/launcher/login",
            200,
            json!({"access_token":"minecraft-secret", "expires_in":3600}),
        );
        self.put(
            "/mc/entitlements/license",
            200,
            if owns {
                json!({"items":[{"name":"game_minecraft"}]})
            } else {
                json!({"items":[{"name":"game_minecraft_bedrock"}]})
            },
        );
        self.put(
            "/mc/minecraft/profile",
            200,
            json!({"id":PROFILE,"name":"TestPlayer"}),
        );
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            w.join().unwrap();
        }
    }
}

#[derive(Default)]
struct Vault(Mutex<BTreeMap<String, RefreshCredential>>);
impl CredentialStore for Vault {
    fn read(&self, id: &str) -> Result<RefreshCredential> {
        self.0
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or("missing".into())
    }
    fn write(&self, id: &str, value: &RefreshCredential) -> Result<()> {
        self.0.lock().unwrap().insert(id.into(), value.clone());
        Ok(())
    }
    fn remove(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
}
fn account() -> Account {
    Account {
        id: PROFILE.into(),
        name: "TestPlayer".into(),
        xuid: "12345".into(),
        client_id: DEFAULT_CLIENT_ID.into(),
    }
}

#[test]
fn device_flow_handles_pending_and_obtains_owned_java_profile() {
    let server = Server::new();
    server.device();
    server.put("/ms/token", 400, json!({"error":"authorization_pending"}));
    server.put(
        "/ms/token",
        200,
        json!({"access_token":"microsoft-secret","refresh_token":"refresh-secret"}),
    );
    server.exchange(true);
    let session = server
        .client
        .login(
            DEFAULT_CLIENT_ID,
            &AtomicBool::new(false),
            |code| assert_eq!(code.user_code, "USER-CODE"),
            |_| {},
        )
        .unwrap();
    assert_eq!(session.account, account());
    assert_eq!(session.access_token, "minecraft-secret");
    assert!(session.expires_at > now() + 3500);
    let requests = server.requests.lock().unwrap();
    let xbox: Value =
        serde_json::from_str(&requests.iter().find(|r| r.0 == "/xbox").unwrap().1).unwrap();
    assert_eq!(xbox["Properties"]["RpsTicket"], "d=microsoft-secret");
    let list = AccountList {
        accounts: vec![session.account],
        selected: Some(PROFILE.into()),
    };
    let metadata = serde_json::to_string(&list).unwrap();
    assert!(!metadata.contains("secret"));
    assert!(!metadata.contains("token"));
}

#[test]
fn cancellation_never_polls_or_creates_a_session() {
    let server = Server::new();
    server.device();
    let cancel = AtomicBool::new(false);
    let result = server.client.login(
        DEFAULT_CLIENT_ID,
        &cancel,
        |_| cancel.store(true, Ordering::Relaxed),
        |_| {},
    );
    assert_eq!(result.err().unwrap(), "登录已取消。");
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[test]
fn slow_down_increases_all_subsequent_poll_intervals() {
    let server = Server::new();
    server.device();
    server.put("/ms/token", 400, json!({"error":"slow_down"}));
    server.put("/ms/token", 400, json!({"error":"authorization_declined"}));
    let start = Instant::now();
    let result = server
        .client
        .login(DEFAULT_CLIENT_ID, &AtomicBool::new(false), |_| {}, |_| {});
    assert!(result.err().unwrap().contains("拒绝"));
    assert!(start.elapsed() >= Duration::from_secs(7));
}

#[test]
fn expired_device_code_stops_without_polling() {
    let server = Server::new();
    server.put("/ms/devicecode", 200, json!({"device_code":"device", "user_code":"code", "verification_uri":"https://www.microsoft.com/link", "expires_in":1, "interval":5}));
    assert!(
        server
            .client
            .login(DEFAULT_CLIENT_ID, &AtomicBool::new(false), |_| {}, |_| {})
            .err()
            .unwrap()
            .contains("过期")
    );
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[test]
fn bedrock_only_entitlements_do_not_allow_java_login() {
    let server = Server::new();
    server.exchange(false);
    let result = server.client.exchange(
        DEFAULT_CLIENT_ID,
        MicrosoftToken {
            access_token: "ms".into(),
            refresh_token: "refresh".into(),
        },
        &AtomicBool::new(false),
        &|_| {},
    );
    assert!(result.err().unwrap().contains("没有有效"));
    assert!(
        !server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.0.ends_with("/profile"))
    );
}

#[test]
fn rotated_refresh_is_saved_even_if_minecraft_service_rejects_app() {
    let server = Server::new();
    server.exchange(true);
    server.routes.lock().unwrap().remove("/mc/launcher/login");
    server.put(
        "/mc/launcher/login",
        403,
        json!({"message":"Invalid app registration", "debug":"DO-NOT-LEAK-TOKEN"}),
    );
    server.put(
        "/ms/token",
        200,
        json!({"access_token":"ms","refresh_token":"rotated-refresh"}),
    );
    let vault = Vault::default();
    vault
        .write(
            PROFILE,
            &RefreshCredential {
                client_id: DEFAULT_CLIENT_ID.into(),
                refresh_token: "old".into(),
            },
        )
        .unwrap();
    let error = server
        .client
        .resume(&account(), None, &vault, &AtomicBool::new(false))
        .err()
        .unwrap();
    assert!(error.contains("403"));
    assert!(!error.contains("DO-NOT-LEAK"));
    assert_eq!(
        vault.read(PROFILE).unwrap().refresh_token,
        "rotated-refresh"
    );
}

#[test]
fn xbox_restrictions_are_explained_without_echoing_sensitive_response() {
    let server = Server::new();
    server.put(
        "/xbox",
        401,
        json!({"XErr":2148916238u64,"Token":"do-not-leak"}),
    );
    let error = server
        .client
        .exchange(
            DEFAULT_CLIENT_ID,
            MicrosoftToken {
                access_token: "ms".into(),
                refresh_token: "refresh".into(),
            },
            &AtomicBool::new(false),
            &|_| {},
        )
        .err()
        .unwrap();
    assert!(error.contains("家庭权限"));
    assert!(!error.contains("do-not-leak"));
    assert!(validate_client_id("not-a-client-id").is_err());
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
#[ignore = "explicit test of an isolated dummy system credential"]
fn system_vault_roundtrip() {
    let id = uuid::Uuid::new_v4().simple().to_string();
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = SystemVault.remove(&self.0);
        }
    }
    let _cleanup = Cleanup(id.clone());
    let credential = RefreshCredential {
        client_id: DEFAULT_CLIENT_ID.into(),
        refresh_token: "NSCL-test-dummy-credential".into(),
    };
    SystemVault.write(&id, &credential).unwrap();
    assert_eq!(
        SystemVault.read(&id).unwrap().refresh_token,
        credential.refresh_token
    );
    SystemVault.remove(&id).unwrap();
    assert!(SystemVault.read(&id).is_err());
}

#[test]
fn legacy_api_compatibility_is_used_only_when_endpoint_is_absent() {
    let server = Server::new();
    server.exchange(true);
    server.routes.lock().unwrap().remove("/mc/launcher/login");
    server
        .routes
        .lock()
        .unwrap()
        .remove("/mc/entitlements/license");
    server.put(
        "/mc/authentication/login_with_xbox",
        200,
        json!({"access_token":"legacy-token","expires_in":3600}),
    );
    server.put(
        "/mc/entitlements/mcstore",
        200,
        json!({"items":[{"name":"product_minecraft"}]}),
    );
    let session = server
        .client
        .exchange(
            DEFAULT_CLIENT_ID,
            MicrosoftToken {
                access_token: "ms".into(),
                refresh_token: "refresh".into(),
            },
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();
    assert_eq!(session.access_token, "legacy-token");
    let requests = server.requests.lock().unwrap();
    let body: Value = serde_json::from_str(
        &requests
            .iter()
            .find(|r| r.0.ends_with("/login_with_xbox"))
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(body["identityToken"], "XBL3.0 x=hash;xsts-token");
}

#[test]
fn rejected_cached_token_is_refreshed_once_then_revalidated() {
    let server = Server::new();
    server.put(
        "/mc/entitlements/license",
        401,
        json!({"error":"UnauthorizedOperationException"}),
    );
    server.put(
        "/ms/token",
        200,
        json!({"access_token":"new-ms","refresh_token":"new-refresh"}),
    );
    server.exchange(true);
    let cached = Session {
        account: account(),
        credential: RefreshCredential {
            client_id: DEFAULT_CLIENT_ID.into(),
            refresh_token: "old-refresh".into(),
        },
        access_token: "expired-early".into(),
        expires_at: now() + 1000,
    };
    let vault = Vault::default();
    let session = server
        .client
        .resume(&account(), Some(cached), &vault, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(session.access_token, "minecraft-secret");
    assert_eq!(vault.read(PROFILE).unwrap().refresh_token, "new-refresh");
    assert_eq!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.0 == "/ms/token")
            .count(),
        1
    );
}
