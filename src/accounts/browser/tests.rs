use super::*;
use std::sync::{Arc, Mutex};

fn redirect() -> String {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    format!(
        "http://localhost:{}/auth-response",
        listener.local_addr().unwrap().port()
    )
}
fn authorization() -> BrowserAuthorization {
    BrowserAuthorization::new(DEFAULT_CLIENT_ID, &redirect()).unwrap()
}
fn request(auth: &BrowserAuthorization, target: &str) -> String {
    let redirect = reqwest::Url::parse(&auth.prompt.redirect_uri).unwrap();
    format!(
        "GET {target} HTTP/1.1\r\nHost: localhost:{}\r\nUser-Agent: test\r\n",
        redirect.port().unwrap()
    )
}

#[test]
fn pkce_matches_rfc7636_and_authorization_uses_registered_native_redirect() {
    assert_eq!(
        challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    let auth = authorization();
    let url = reqwest::Url::parse(&auth.prompt.url).unwrap();
    let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(url.host_str(), Some("login.microsoftonline.com"));
    assert_eq!(query["client_id"], DEFAULT_CLIENT_ID);
    assert_eq!(query["redirect_uri"], auth.prompt.redirect_uri);
    assert_eq!(query["response_type"], "code");
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["code_challenge"], challenge(&auth.verifier));
    assert_eq!(auth.verifier.len(), 43);
    assert_ne!(auth.verifier, auth.state);
    assert!(!auth.prompt.url.contains(&auth.verifier));
    assert!(
        auth.listeners
            .iter()
            .all(|l| l.local_addr().unwrap().ip().is_loopback())
    );
}

#[test]
fn callbacks_require_expected_path_host_and_unique_matching_state() {
    let auth = authorization();
    let valid = format!("/auth-response?code=test-code&state={}", auth.state);
    assert!(
        matches!(auth.callback(&request(&auth,&valid)), Callback::Complete(Ok(code)) if code == "test-code")
    );
    for target in [
        "/favicon.ico?code=test&state=wrong".to_owned(),
        "/auth-response?code=test&state=wrong".to_owned(),
        "/auth-response?code=test".to_owned(),
        format!(
            "/auth-response?state={}&state={}&code=test",
            auth.state, auth.state
        ),
        format!(
            "/auth-response?state={}&code=test&error=access_denied",
            auth.state
        ),
        format!(
            "//evil.example/auth-response?code=test&state={}",
            auth.state
        ),
        format!(
            "/\\evil.example/auth-response?code=test&state={}",
            auth.state
        ),
    ] {
        assert!(matches!(
            auth.callback(&request(&auth, &target)),
            Callback::Ignore(_)
        ));
    }
    let evil_host = request(&auth, &valid).replace("Host: localhost:", "Host: evil.example:");
    assert!(matches!(auth.callback(&evil_host), Callback::Ignore(400)));
    assert!(matches!(
        auth.callback(&request(&auth, &valid).replace("GET ", "POST ")),
        Callback::Ignore(400)
    ));
}

#[test]
fn cancellation_timeout_and_denied_authorization_do_not_produce_codes() {
    let mut auth = authorization();
    assert_eq!(
        auth.wait_for_code(&AtomicBool::new(true)).unwrap_err(),
        "登录已取消。"
    );
    auth.prompt.expires = Instant::now();
    assert!(
        auth.wait_for_code(&AtomicBool::new(false))
            .unwrap_err()
            .contains("超时")
    );
    let target = format!("/auth-response?error=access_denied&state={}", auth.state);
    assert!(
        matches!(auth.callback(&request(&auth,&target)),Callback::Complete(Err(error)) if error.contains("拒绝"))
    );
    let error = oauth_response_error(
        &json!({"error":"invalid_client", "error_description":"AADSTS7000218: RAW-PRIVATE-INFORMATION"}),
    );
    assert!(error.contains("AADSTS7000218"));
    assert!(error.contains("移动和桌面"));
    assert!(!error.contains("RAW-PRIVATE"));
}

#[test]
fn redirect_configuration_rejects_remote_and_ambiguous_urls() {
    assert!(validate_redirect_uri(DEFAULT_REDIRECT_URI).is_ok());
    assert!(validate_redirect_uri("http://127.0.0.1:29111/auth-response").is_ok());
    assert!(validate_redirect_uri("http://[::1]:29111/auth-response").is_ok());
    for uri in [
        "https://evil.example/callback",
        "http://0.0.0.0:29111/auth-response",
        "http://localhost:29111/",
        "http://localhost:80/auth-response",
        "http://localhost:29111/auth-response?code=x",
        "http://localhost:29111/auth-response#state=x",
        "http://user@localhost:29111/auth-response",
    ] {
        assert!(validate_redirect_uri(uri).is_err());
    }
}

#[test]
fn browser_flow_exchanges_authorization_code_with_pkce_and_exact_redirect() {
    let server = super::super::tests::Server::new();
    server.put(
        "/ms/token",
        200,
        json!({"access_token":"fake-microsoft-token","refresh_token":"fake-refresh"}),
    );
    server.exchange(true);
    let cancel = AtomicBool::new(false);
    let received = Arc::new(Mutex::new(None));
    let redirect = redirect();
    let session = std::thread::scope(|scope| {
        server
            .client
            .login_browser(
                DEFAULT_CLIENT_ID,
                &redirect,
                &cancel,
                |prompt| {
                    *received.lock().unwrap() = Some(prompt.clone());
                    let prompt = prompt.clone();
                    scope.spawn(move || {
                        let url = reqwest::Url::parse(&prompt.url).unwrap();
                        let state = url
                            .query_pairs()
                            .find(|(key, _)| key == "state")
                            .unwrap()
                            .1
                            .into_owned();
                        let client = Client::builder()
                            .no_proxy()
                            .timeout(Duration::from_secs(3))
                            .build()
                            .unwrap();
                        let favicon = format!(
                            "{}favicon.ico",
                            prompt.redirect_uri.trim_end_matches("auth-response")
                        );
                        assert_eq!(client.get(favicon).send().unwrap().status().as_u16(), 404);
                        let response = client
                            .get(&prompt.redirect_uri)
                            .query(&[
                                ("state", state.as_str()),
                                ("code", "fake-authorization-code"),
                            ])
                            .send()
                            .unwrap();
                        assert!(response.status().is_success());
                        assert_eq!(response.headers()["referrer-policy"], "no-referrer");
                        let html = response.text().unwrap();
                        assert!(!html.contains("fake-authorization-code"));
                        assert!(!html.contains(&state));
                    });
                },
                |_| {},
            )
            .unwrap()
    });
    assert_eq!(session.account.name, "TestPlayer");
    let requests = server.requests.lock().unwrap();
    let body = &requests.iter().find(|r| r.0 == "/ms/token").unwrap().1;
    let fields: std::collections::BTreeMap<_, _> =
        reqwest::Url::parse(&format!("http://test/?{body}"))
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect();
    assert_eq!(fields["grant_type"], "authorization_code");
    assert_eq!(fields["code"], "fake-authorization-code");
    assert_eq!(fields["redirect_uri"], redirect);
    let prompt = received.lock().unwrap().take().unwrap();
    let url = reqwest::Url::parse(&prompt.url).unwrap();
    let expected = url
        .query_pairs()
        .find(|(key, _)| key == "code_challenge")
        .unwrap()
        .1
        .into_owned();
    assert_eq!(challenge(&fields["code_verifier"]), expected);
    assert!(!fields.contains_key("client_secret"));
    assert!(
        TcpListener::bind((
            Ipv4Addr::LOCALHOST,
            reqwest::Url::parse(&redirect).unwrap().port().unwrap()
        ))
        .is_ok()
    );
}

#[test]
fn occupied_custom_callback_port_returns_an_actionable_error() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let redirect = format!(
        "http://127.0.0.1:{}/auth-response",
        listener.local_addr().unwrap().port()
    );
    let result = BrowserAuthorization::new(DEFAULT_CLIENT_ID, &redirect);
    assert!(result.err().unwrap().contains("端口被占用"));
}
