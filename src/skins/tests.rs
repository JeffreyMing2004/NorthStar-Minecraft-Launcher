use super::*;
use crate::accounts::{Account, RefreshCredential};

#[test]
fn default_is_embedded_classic_steve_with_transparent_outer_layer() {
    let skin = default_skin();
    assert_eq!(skin.model, Model::Classic);
    assert_eq!(skin.pixels.dimensions(), (64, 64));
    let shirt = skin.pixels.get_pixel(22, 23).0;
    assert!(shirt[1] > shirt[0] && shirt[2] > shirt[0]);
    assert_eq!(skin.pixels.get_pixel(10, 10).0[3], 255);
    assert_eq!(skin.pixels.get_pixel(40, 8).0[3], 0);
}

fn bytes(width: u32, height: u32) -> Vec<u8> {
    let pixels = RgbaImage::from_pixel(width, height, image::Rgba([20, 60, 100, 255]));
    let mut data = Cursor::new(Vec::new());
    pixels.write_to(&mut data, ImageFormat::Png).unwrap();
    data.into_inner()
}
fn session() -> Session {
    Session {
        account: Account {
            kind: crate::accounts::AccountKind::Microsoft,
            id: "0123456789abcdef0123456789abcdef".into(),
            name: "Test".into(),
            xuid: "1".into(),
            client_id: crate::accounts::DEFAULT_CLIENT_ID.into(),
        },
        credential: RefreshCredential {
            client_id: crate::accounts::DEFAULT_CLIENT_ID.into(),
            refresh_token: "private-refresh".into(),
        },
        access_token: "private-access".into(),
        expires_at: u64::MAX,
    }
}

#[test]
fn validates_png_sizes_and_normalizes_legacy_limbs() {
    for size in [(64, 64), (64, 32)] {
        assert_eq!(
            png(&bytes(size.0, size.1), Model::Classic)
                .unwrap()
                .pixels
                .dimensions(),
            (64, 64)
        );
    }
    assert!(png(&bytes(64, 32), Model::Slim).is_err());
    for size in [(32, 32), (128, 128), (64, 16)] {
        assert!(png(&bytes(size.0, size.1), Model::Classic).is_err());
    }
    assert!(png(b"not png", Model::Classic).is_err());
    assert!(png(&vec![0; MAX_BYTES as usize + 1], Model::Classic).is_err());
    let legacy = png(&bytes(64, 32), Model::Classic).unwrap();
    assert_eq!(legacy.pixels.get_pixel(20, 52).0, [20, 60, 100, 255]);
    assert_eq!(legacy.pixels.get_pixel(40, 52).0, [20, 60, 100, 255]);
    assert_eq!(legacy.pixels.get_pixel(35, 4)[3], 0);
    assert_eq!(legacy.pixels.get_pixel(20, 36)[3], 0);
}

#[test]
fn gets_skin_without_leaking_token_to_texture_request_and_uploads_multipart() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr());
    let mut client = SkinClient::new().unwrap();
    client.base = base.clone();
    let texture = bytes(64, 64);
    let profile=serde_json::json!({"id":session().account.id,"skins":[{"state":"ACTIVE","url":format!("{base}/texture"),"variant":"SLIM"}]}).to_string();
    let worker = std::thread::spawn(move || {
        for path in [
            "/minecraft/profile",
            "/texture",
            "/minecraft/profile",
            "/minecraft/profile/skins",
        ] {
            let mut request = server
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            assert_eq!(request.url(), path);
            let auth = request
                .headers()
                .iter()
                .find(|h| h.field.equiv("Authorization"));
            if path == "/texture" {
                assert!(auth.is_none());
            } else {
                assert_eq!(auth.unwrap().value.as_str(), "Bearer private-access");
            }
            if path.ends_with("/skins") {
                assert_eq!(request.method(), &tiny_http::Method::Post);
                let mut body = Vec::new();
                request.as_reader().read_to_end(&mut body).unwrap();
                let text = String::from_utf8_lossy(&body);
                assert!(text.contains("name=\"variant\""));
                assert!(text.contains("slim"));
                assert!(text.contains("filename=\"skin.png\""));
                assert!(body.windows(8).any(|w| w == b"\x89PNG\r\n\x1a\n"));
            }
            let response = if path == "/texture" {
                tiny_http::Response::from_data(texture.clone())
            } else {
                tiny_http::Response::from_string(&profile)
            };
            request.respond(response).unwrap();
        }
    });
    assert_eq!(client.current(&session()).unwrap().model, Model::Slim);
    client
        .upload(&session(), bytes(64, 64), Model::Slim)
        .unwrap();
    worker.join().unwrap();
}

#[test]
fn rejects_wrong_account_and_hides_sensitive_server_errors() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let mut client = SkinClient::new().unwrap();
    client.base = format!("http://{}", server.server_addr());
    let worker = std::thread::spawn(move || {
        let request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        request
            .respond(tiny_http::Response::from_string(
                "{\"id\":\"wrong\",\"skins\":[]}",
            ))
            .unwrap();
        let request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        request
            .respond(
                tiny_http::Response::from_string("private-access private-refresh")
                    .with_status_code(403),
            )
            .unwrap();
    });
    assert!(
        client
            .upload(&session(), bytes(64, 64), Model::Classic)
            .unwrap_err()
            .contains("不一致")
    );
    let error = client.current(&session()).err().unwrap();
    assert!(error.contains("拒绝"));
    assert!(!error.contains("private"));
    worker.join().unwrap();
}

fn cape_profile(base: &str, active: Option<&str>) -> String {
    serde_json::json!({
        "id": session().account.id,
        "capes": [
            {"id":"first", "alias":"First Cape", "state":if active == Some("first") {"ACTIVE"} else {"INACTIVE"}, "url":format!("{base}/cape.png")},
            {"id":"second", "alias":"Second Cape", "state":if active == Some("second") {"ACTIVE"} else {"INACTIVE"}, "url":format!("{base}/cape.png")}
        ]
    }).to_string()
}

#[test]
fn lists_all_owned_capes_and_downloads_previews_without_credentials() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let mut client = SkinClient::new().unwrap();
    client.base = format!("http://{}", server.server_addr());
    let profile = cape_profile(&client.base, Some("first"));
    let worker = std::thread::spawn(move || {
        for path in ["/minecraft/profile", "/cape.png"] {
            let request = server
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            assert_eq!(request.url(), path);
            let auth = request
                .headers()
                .iter()
                .find(|h| h.field.equiv("Authorization"));
            if path.ends_with("png") {
                assert!(auth.is_none());
                request
                    .respond(tiny_http::Response::from_data(bytes(64, 32)))
                    .unwrap();
            } else {
                assert_eq!(auth.unwrap().value.as_str(), "Bearer private-access");
                request
                    .respond(tiny_http::Response::from_string(&profile))
                    .unwrap();
            }
        }
    });
    let capes = client.capes(&session()).unwrap();
    assert_eq!(capes.len(), 2);
    assert!(capes[0].active && !capes[1].active);
    assert_eq!(capes[1].name, "Second Cape");
    assert_eq!(
        client.cape_texture(&capes[1]).unwrap().dimensions(),
        (64, 32)
    );
    let mut untrusted = capes[0].clone();
    untrusted.url = "https://example.invalid/cape.png".into();
    assert!(
        client
            .cape_texture(&untrusted)
            .unwrap_err()
            .contains("不受信任")
    );
    worker.join().unwrap();
}

#[test]
fn selects_owned_capes_and_hides_them_only_after_server_confirmation() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let mut client = SkinClient::new().unwrap();
    client.base = format!("http://{}", server.server_addr());
    let base = client.base.clone();
    let worker = std::thread::spawn(move || {
        use tiny_http::Method::{Delete, Get, Put};
        let calls = [
            (Get, Some("first")),
            (Put, None),
            (Get, Some("second")),
            (Get, Some("second")), // Refuse a cape not owned by this account.
            (Get, Some("second")),
            (Delete, None),
            (Get, None),
            (Get, None), // Already hidden: no duplicate DELETE.
        ];
        for (method, active) in calls {
            let mut request = server
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            assert_eq!(request.method(), &method);
            assert_eq!(
                request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .unwrap()
                    .value
                    .as_str(),
                "Bearer private-access"
            );
            if method == Get {
                assert_eq!(request.url(), "/minecraft/profile");
                request
                    .respond(tiny_http::Response::from_string(cape_profile(
                        &base, active,
                    )))
                    .unwrap();
            } else {
                assert_eq!(request.url(), "/minecraft/profile/capes/active");
                if method == Put {
                    let body: serde_json::Value =
                        serde_json::from_reader(request.as_reader()).unwrap();
                    assert_eq!(body, serde_json::json!({"capeId":"second"}));
                }
                request.respond(tiny_http::Response::empty(204)).unwrap();
            }
        }
    });
    let capes = client.select_cape(&session(), Some("second")).unwrap();
    assert!(!capes[0].active && capes[1].active);
    assert!(
        client
            .select_cape(&session(), Some("not-owned"))
            .unwrap_err()
            .contains("不拥有")
    );
    assert!(
        client
            .select_cape(&session(), None)
            .unwrap()
            .iter()
            .all(|c| !c.active)
    );
    assert!(
        client
            .select_cape(&session(), None)
            .unwrap()
            .iter()
            .all(|c| !c.active)
    );
    worker.join().unwrap();
}

#[test]
fn cape_failures_hide_credentials_and_reject_unconfirmed_changes() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let mut client = SkinClient::new().unwrap();
    client.base = format!("http://{}", server.server_addr());
    let profile = cape_profile(&client.base, Some("first"));
    let worker = std::thread::spawn(move || {
        for code in [200, 429, 200, 204, 200, 200] {
            let request = server
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            let body = if request.url() == "/minecraft/profile" {
                profile.as_str()
            } else {
                "private-access private-refresh"
            };
            request
                .respond(tiny_http::Response::from_string(body).with_status_code(code))
                .unwrap();
        }
    });
    let error = client.select_cape(&session(), Some("second")).unwrap_err();
    assert!(error.contains("频繁") && !error.contains("private"));
    assert!(
        client
            .select_cape(&session(), Some("second"))
            .unwrap_err()
            .contains("尚未更新")
    );
    let mut wrong = session();
    wrong.account.id = "wrong".into();
    assert!(
        client
            .select_cape(&wrong, Some("second"))
            .unwrap_err()
            .contains("不一致")
    );
    worker.join().unwrap();
}

#[test]
fn cape_png_and_profile_state_validation_reject_invalid_previews_and_selection() {
    assert!(cape_png(&bytes(128, 64)).is_ok());
    assert!(cape_png(&bytes(64, 64)).is_err());
    assert!(cape_png(&bytes(32, 16)).is_err());
    assert!(cape_png(&bytes(2048, 1024)).is_err());
    assert!(cape_png(b"not png").is_err());
    assert!(cape_png(&vec![0; MAX_BYTES as usize + 1]).is_err());
    let mut profile: Profile = serde_json::from_str(&cape_profile(
        "https://textures.minecraft.net",
        Some("first"),
    ))
    .unwrap();
    profile.capes[1].state = "ACTIVE".into();
    assert!(profile.capes().is_err());
    let mut profile: Profile =
        serde_json::from_str(&cape_profile("https://textures.minecraft.net", None)).unwrap();
    profile.capes[1].id = "first".into();
    assert!(profile.capes().is_err());
}
