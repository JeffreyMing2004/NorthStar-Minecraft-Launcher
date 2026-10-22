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
