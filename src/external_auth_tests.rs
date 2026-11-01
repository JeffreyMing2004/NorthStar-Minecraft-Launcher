use super::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, atomic::AtomicUsize},
    thread,
};
type Reply = (u16, Vec<u8>, Vec<(&'static str, String)>);
fn reply(value: Value) -> Reply {
    (200, serde_json::to_vec(&value).unwrap(), vec![])
}
fn metadata() -> Reply {
    reply(
        json!({"meta":{"serverName":"Test server"},"skinDomains":[],"signaturePublickey":"-----BEGIN PUBLIC KEY-----\nTEST\n-----END PUBLIC KEY-----"}),
    )
}
fn profile(id: &str) -> Value {
    json!({"id":id,"name":"Ext玩家"})
}
const PROFILE: &str = "0123456789abcdef0123456789abcdef";
struct Server {
    base: String,
    seen: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(handler: impl Fn(&str, &Value) -> Reply + Send + 'static) -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr());
        let stop = Arc::new(AtomicBool::new(false));
        let seen = Arc::new(AtomicUsize::new(0));
        let (flag, count) = (stop.clone(), seen.clone());
        let worker = thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                if let Ok(Some(mut request)) = server.recv_timeout(Duration::from_millis(25)) {
                    count.fetch_add(1, Ordering::Relaxed);
                    let path = request.url().to_owned();
                    let mut bytes = Vec::new();
                    request.as_reader().read_to_end(&mut bytes).unwrap();
                    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                    let (status, bytes, headers) = handler(&path, &value);
                    let mut response =
                        tiny_http::Response::from_data(bytes).with_status_code(status);
                    for (name, value) in headers {
                        response = response
                            .with_header(tiny_http::Header::from_bytes(name, value).unwrap());
                    }
                    let _ = request.respond(response);
                }
            }
        });
        Self {
            base,
            seen,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn temporary() -> tempfile::TempDir {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    std::fs::create_dir_all(&p).unwrap();
    tempfile::tempdir_in(p).unwrap()
}
#[derive(Default)]
struct MemoryVault(Mutex<BTreeMap<String, Credential>>);
impl Vault for MemoryVault {
    fn read(&self, id: &str) -> Result<Credential> {
        self.0
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or("missing".into())
    }
    fn write(&self, id: &str, c: &Credential) -> Result<()> {
        self.0.lock().unwrap().insert(id.into(), c.clone());
        Ok(())
    }
    fn remove(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
}
#[test]
fn discovery_profile_selection_and_account_scope_preserve_identity_without_serializing_secrets() {
    let handler = |path: &str, body: &Value| match path {
        "/start/" => (302, vec![], vec![("Location", "/landing/".into())]),
        "/landing/" => (
            200,
            vec![],
            vec![("X-Authlib-Injector-API-Location", "/api/".into())],
        ),
        "/api/" => metadata(),
        "/api/authserver/authenticate" => {
            assert_eq!(body["agent"]["name"], "Minecraft");
            assert_eq!(body["requestUser"], true);
            reply(
                json!({"accessToken":"SECRET_ACCESS_TOKEN","clientToken":body["clientToken"],"availableProfiles":[profile(PROFILE),profile("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")]}),
            )
        }
        "/api/authserver/refresh" => reply(
            json!({"accessToken":"ROTATED_TOKEN","clientToken":body["clientToken"],"selectedProfile":body["selectedProfile"]}),
        ),
        _ => (404, vec![], vec![]),
    };
    let a = Server::new(handler);
    let b = Server::new(handler);
    let client = Client::new().unwrap();
    let cancel = AtomicBool::new(false);
    let server = client
        .discover(&format!("{}/start", a.base), &cancel)
        .unwrap();
    assert_eq!(server.root, format!("{}/api/", a.base));
    let login = client
        .login(
            &server,
            "account@example.com",
            "PASSWORD_NEVER_STORE",
            &cancel,
        )
        .unwrap();
    assert!(!login.selected());
    assert_eq!(login.profiles.len(), 2);
    assert!(
        client
            .select(
                &login,
                Some(&Profile {
                    id: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                    name: "Other".into()
                }),
                &cancel
            )
            .is_err()
    );
    let session = client
        .select(&login, Some(&login.profiles[0]), &cancel)
        .unwrap();
    let other_server = client
        .discover(&format!("{}/api", b.base), &cancel)
        .unwrap();
    let other_login = client
        .login(
            &other_server,
            "account@example.com",
            "PASSWORD_NEVER_STORE",
            &cancel,
        )
        .unwrap();
    let other = client
        .select(&other_login, Some(&other_login.profiles[0]), &cancel)
        .unwrap();
    assert_ne!(session.account.id, other.account.id);
    assert_eq!(session.account.external.as_ref().unwrap().profile, PROFILE);
    let list = crate::accounts::AccountList {
        accounts: vec![session.account.clone(), other.account],
        selected: Some(session.account.id.clone()),
    };
    list.validate().unwrap();
    let json = serde_json::to_string(&list).unwrap();
    for secret in [
        "SECRET_ACCESS_TOKEN",
        "ROTATED_TOKEN",
        "PASSWORD_NEVER_STORE",
    ] {
        assert!(!json.contains(secret));
    }
    let identity = crate::minecraft::LaunchIdentity::external(
        &session,
        Path::new("H:/game with spaces/authlib.jar"),
    )
    .unwrap();
    assert_eq!(identity.uuid, PROFILE);
    assert_eq!(identity.user_type, "mojang");
    assert!(identity.extra_jvm[0].contains("game with spaces/authlib.jar="));
    assert!(identity.extra_jvm[1].starts_with("-Dauthlibinjector.yggdrasil.prefetched="));
}
#[test]
fn refreshed_tokens_are_saved_and_a_different_profile_cannot_replace_the_selected_identity() {
    let wrong = Arc::new(AtomicBool::new(false));
    let flag = wrong.clone();
    let server = Server::new(move |path, body| match path {
        "/api/" => metadata(),
        "/api/authserver/authenticate" => reply(
            json!({"accessToken":"INITIAL","clientToken":body["clientToken"],"selectedProfile":profile(PROFILE)}),
        ),
        "/api/authserver/refresh" => {
            assert!(body.get("selectedProfile").is_none());
            reply(
                json!({"accessToken":"REFRESHED","clientToken":body["clientToken"],"selectedProfile":profile(if flag.load(Ordering::Relaxed){"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}else{PROFILE})}),
            )
        }
        _ => (404, vec![], vec![]),
    });
    let client = Client::new().unwrap();
    let cancel = AtomicBool::new(false);
    let api = client
        .discover(&format!("{}/api/", server.base), &cancel)
        .unwrap();
    let login = client.login(&api, "user", "password", &cancel).unwrap();
    let session = client.select(&login, None, &cancel).unwrap();
    let vault = MemoryVault::default();
    vault
        .write(&session.account.id, &session.credential)
        .unwrap();
    let refreshed = client
        .resume(&session.account, None, &vault, &cancel)
        .unwrap();
    assert_eq!(refreshed.credential.access_token, "REFRESHED");
    assert_eq!(
        vault.read(&session.account.id).unwrap().access_token,
        "REFRESHED"
    );
    wrong.store(true, Ordering::Relaxed);
    assert!(
        client
            .resume(&session.account, Some(refreshed), &vault, &cancel)
            .err()
            .unwrap()
            .contains("身份")
    );
    assert_eq!(
        vault.read(&session.account.id).unwrap().access_token,
        "REFRESHED"
    );
}
#[test]
fn credential_posts_never_follow_redirects_errors_hide_secrets_and_cancel_prevents_network() {
    let target = Server::new(|_, _| reply(json!({})));
    let location = format!("{}/capture", target.base);
    let server = Server::new(move |path, _| {
        if path == "/api/" {
            metadata()
        } else {
            (
                302,
                b"PASSWORD_NEVER_ECHO".to_vec(),
                vec![("Location", location.clone())],
            )
        }
    });
    let client = Client::new().unwrap();
    let cancel = AtomicBool::new(false);
    let api = client
        .discover(&format!("{}/api/", server.base), &cancel)
        .unwrap();
    let error = client
        .login(&api, "user", "PASSWORD_NEVER_ECHO", &cancel)
        .err()
        .unwrap();
    assert!(error.contains("302"));
    assert!(!error.contains("PASSWORD_NEVER_ECHO"));
    assert_eq!(target.seen.load(Ordering::Relaxed), 0);
    let seen = server.seen.load(Ordering::Relaxed);
    cancel.store(true, Ordering::Relaxed);
    assert!(client.login(&api, "user", "password", &cancel).is_err());
    assert_eq!(server.seen.load(Ordering::Relaxed), seen);
    for value in [
        "http://example.com/",
        "https://user:password@example.com/",
        "https://example.com/?token=x",
        "file:///tmp/",
    ] {
        assert!(url(value).is_err());
    }
}
#[test]
fn injector_artifacts_are_hash_verified_cached_and_repaired_without_trusting_changed_bytes() {
    let bytes = b"TEST_JAR_CONTENT";
    let hash = format!("{:x}", Sha256::digest(bytes));
    let address = Arc::new(Mutex::new(String::new()));
    let url = address.clone();
    let bad = Arc::new(AtomicBool::new(false));
    let mode = bad.clone();
    let server = Server::new(move |path, _| {
        if path == "/manifest" {
            reply(
                json!({"build_number":1,"download_url":format!("{}/agent.jar",url.lock().unwrap()),"checksums":{"sha256":hash}}),
            )
        } else {
            (
                200,
                if mode.load(Ordering::Relaxed) {
                    b"BAD".to_vec()
                } else {
                    bytes.to_vec()
                },
                vec![],
            )
        }
    });
    *address.lock().unwrap() = server.base.clone();
    let root = temporary();
    let d = Downloader::new(
        crate::minecraft::Source {
            proxy_mode: crate::minecraft::ProxyMode::Direct,
            ..Default::default()
        },
        Arc::new(AtomicBool::new(false)),
        Arc::new(|_| {}),
    )
    .unwrap();
    let path = install_agent_from(root.path(), &d, &format!("{}/manifest", server.base)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let calls = server.seen.load(Ordering::Relaxed);
    assert_eq!(
        install_agent_from(root.path(), &d, &format!("{}/manifest", server.base)).unwrap(),
        path
    );
    assert_eq!(server.seen.load(Ordering::Relaxed), calls);
    std::fs::write(&path, b"changed locally").unwrap();
    install_agent_from(root.path(), &d, &format!("{}/manifest", server.base)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let broken = temporary();
    bad.store(true, Ordering::Relaxed);
    assert!(
        install_agent_from(broken.path(), &d, &format!("{}/manifest", server.base))
            .err()
            .unwrap()
            .contains("SHA-256")
    );
    assert!(
        !broken
            .path()
            .join("authlib-injector/artifact.json")
            .exists()
    );
}

#[test]
fn injector_refreshes_daily_preserves_verified_versions_on_failure_and_never_ignores_cancellation()
{
    let address = Arc::new(Mutex::new(String::new()));
    let url = address.clone();
    let mode = Arc::new(AtomicUsize::new(1));
    let response_mode = mode.clone();
    let server = Server::new(move |path, _| {
        let mode = response_mode.load(Ordering::Relaxed);
        if path == "/manifest" {
            if mode == 3 {
                return (503, b"offline".to_vec(), vec![]);
            }
            let build = if mode == 4 { 3 } else { mode };
            let bytes = format!("agent build {build}");
            reply(
                json!({"build_number":build,"download_url":format!("{}/agent-{build}.jar",url.lock().unwrap()),"checksums":{"sha256":format!("{:x}",Sha256::digest(bytes.as_bytes()))}}),
            )
        } else if mode == 4 {
            (200, b"CORRUPT_NEW_DOWNLOAD".to_vec(), vec![])
        } else {
            let build = path.trim_start_matches("/agent-").trim_end_matches(".jar");
            (200, format!("agent build {build}").into_bytes(), vec![])
        }
    });
    *address.lock().unwrap() = server.base.clone();
    let root = temporary();
    let cancel = Arc::new(AtomicBool::new(false));
    let d = Downloader::new(
        crate::minecraft::Source {
            proxy_mode: crate::minecraft::ProxyMode::Direct,
            ..Default::default()
        },
        cancel.clone(),
        Arc::new(|_| {}),
    )
    .unwrap();
    let manifest = format!("{}/manifest", server.base);
    let start = 1_000_000;
    let v1 = install_agent_at(root.path(), &d, &manifest, start).unwrap();
    mode.store(2, Ordering::Relaxed);
    let seen = server.seen.load(Ordering::Relaxed);
    assert_eq!(
        install_agent_at(root.path(), &d, &manifest, start + AGENT_CHECK_INTERVAL - 1).unwrap(),
        v1
    );
    assert_eq!(server.seen.load(Ordering::Relaxed), seen);
    let refreshed_at = start + AGENT_CHECK_INTERVAL;
    let v2 = install_agent_at(root.path(), &d, &manifest, refreshed_at).unwrap();
    assert_ne!(v1, v2);
    assert_eq!(std::fs::read(&v2).unwrap(), b"agent build 2");
    assert!(v1.exists());
    let failed_at = refreshed_at + AGENT_CHECK_INTERVAL;
    mode.store(3, Ordering::Relaxed);
    assert_eq!(
        install_agent_at(root.path(), &d, &manifest, failed_at).unwrap(),
        v2
    );
    let seen = server.seen.load(Ordering::Relaxed);
    assert_eq!(
        install_agent_at(root.path(), &d, &manifest, failed_at + 1).unwrap(),
        v2
    );
    assert_eq!(server.seen.load(Ordering::Relaxed), seen);
    mode.store(4, Ordering::Relaxed);
    let invalid_at = failed_at + AGENT_RETRY_INTERVAL;
    assert_eq!(
        install_agent_at(root.path(), &d, &manifest, invalid_at).unwrap(),
        v2
    );
    let record = root.path().join("authlib-injector/artifact.json");
    let stored: AgentCache = serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    assert_eq!(stored.agent.build_number, 2);
    mode.store(1, Ordering::Relaxed);
    assert_eq!(
        install_agent_at(
            root.path(),
            &d,
            &manifest,
            invalid_at + AGENT_RETRY_INTERVAL
        )
        .unwrap(),
        v2
    );
    cancel.store(true, Ordering::Relaxed);
    assert!(install_agent_at(root.path(), &d, &manifest, invalid_at + 1).is_err());
    cancel.store(false, Ordering::Relaxed);
    std::fs::write(&v2, b"corrupt cached jar").unwrap();
    mode.store(3, Ordering::Relaxed);
    assert!(
        install_agent_at(
            root.path(),
            &d,
            &manifest,
            invalid_at + AGENT_RETRY_INTERVAL + 1
        )
        .is_err()
    );
    // Old releases stored the bare descriptor. It must trigger a check and remain
    // usable offline only if its JAR still passes the original SHA-256 check.
    std::fs::write(&v2, b"agent build 2").unwrap();
    crate::store::write_json(&record, &stored.agent).unwrap();
    let seen = server.seen.load(Ordering::Relaxed);
    assert_eq!(
        install_agent_at(root.path(), &d, &manifest, invalid_at).unwrap(),
        v2
    );
    assert!(server.seen.load(Ordering::Relaxed) > seen);
    mode.store(2, Ordering::Relaxed);
    assert_eq!(
        install_agent_at(
            root.path(),
            &d,
            &manifest,
            invalid_at + AGENT_RETRY_INTERVAL
        )
        .unwrap(),
        v2
    );
}
