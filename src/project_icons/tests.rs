use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

fn directory() -> tempfile::TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&parent).unwrap();
    tempfile::tempdir_in(parent).unwrap()
}

fn encoded(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        width,
        height,
        image::Rgb([25, 170, 100]),
    ));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, format).unwrap();
    bytes.into_inner()
}

struct Server {
    url: String,
    requests: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn new(bytes: Vec<u8>) -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/icon", server.server_addr());
        let requests = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let (seen, stop) = (requests.clone(), stopped.clone());
        let thread = std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(20)) {
                    seen.fetch_add(1, Ordering::Relaxed);
                    let _ = request.respond(tiny_http::Response::from_data(bytes.clone()));
                }
            }
        });
        Self {
            url,
            requests,
            stopped,
            thread: Some(thread),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn wait_for_icon(icons: &mut ProjectIcons, ctx: &Context, url: &str) -> Logo {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let logo = icons.get(ctx, Some(url));
        if icons.active == 0 {
            return logo;
        }
        assert!(Instant::now() < deadline, "logo worker did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn raster_formats_keep_proportions_and_transparency() {
    for format in [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::WebP,
        ImageFormat::Gif,
    ] {
        let image = decode(&encoded(format, 320, 160)).unwrap();
        assert_eq!(image.dimensions(), (128, 64), "{format:?}");
    }
    let image = image::RgbaImage::from_pixel(16, 16, image::Rgba([10, 20, 30, 0]));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    assert!(decode(bytes.get_ref()).unwrap().pixels().all(|p| p[3] == 0));
}

#[test]
fn logo_paint_centers_the_image_without_stretching() {
    let ctx = Context::default();
    let texture = ctx.load_texture(
        "wide-logo",
        ColorImage::filled([128, 64], egui::Color32::WHITE),
        TextureOptions::LINEAR,
    );
    let logo = Logo(Some(texture.clone()));
    let bounds = Rect::from_min_size(egui::pos2(20.0, 20.0), egui::Vec2::splat(56.0));
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        logo.paint(ui, bounds, "Wide")
    });
    let image_rect = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect
                    .brush
                    .as_ref()
                    .is_some_and(|brush| brush.fill_texture_id == texture.id()) =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .expect("logo texture was not painted");
    assert_eq!(image_rect.size(), egui::vec2(56.0, 28.0));
    assert_eq!(image_rect.center(), bounds.center());
    output.drop_without_applying_deltas();
}

#[test]
fn invalid_oversized_images_and_untrusted_urls_are_rejected() {
    assert!(decode(b"not an image").is_err());
    assert!(decode(&encoded(ImageFormat::Png, 4097, 1)).is_err());
    assert!(read_limited(Cursor::new(vec![0; MAX_BYTES as usize + 1])).is_err());
    assert!(allowed_url("https://cdn.modrinth.com/data/test/icon.png"));
    for url in [
        "",
        "file:///logo.png",
        "http://cdn.modrinth.com/logo.png",
        "https://cdn.modrinth.com.evil.example/logo.png",
        "https://example.com/logo.png",
        "https://user:secret@cdn.modrinth.com/logo.png",
        "https://cdn.modrinth.com:999/logo.png",
    ] {
        assert!(!allowed_url(url), "{url}");
    }
}

#[test]
fn concurrent_lookups_are_deduplicated_and_disk_cache_survives_a_restart() {
    let root = directory();
    let server = Server::new(encoded(ImageFormat::WebP, 320, 160));
    let ctx = Context::default();
    let mut icons = ProjectIcons::new(root.path().into());
    for _ in 0..10 {
        icons.get(&ctx, Some(&server.url));
    }
    let texture = wait_for_icon(&mut icons, &ctx, &server.url).0.unwrap();
    assert_eq!(texture.size(), [128, 64]);
    assert_eq!(server.requests.load(Ordering::Relaxed), 1);
    let mut restarted = ProjectIcons::new(root.path().into());
    assert!(wait_for_icon(&mut restarted, &ctx, &server.url).0.is_some());
    assert_eq!(server.requests.load(Ordering::Relaxed), 1);
    // A broken cache is recovered from the source, rather than hiding the logo forever.
    fs::write(
        root.path().join(format!("{}.png", cache_key(&server.url))),
        b"broken",
    )
    .unwrap();
    let mut restarted = ProjectIcons::new(root.path().into());
    assert!(wait_for_icon(&mut restarted, &ctx, &server.url).0.is_some());
    assert_eq!(server.requests.load(Ordering::Relaxed), 2);
    assert!(
        decode(&fs::read(root.path().join(format!("{}.png", cache_key(&server.url)))).unwrap())
            .is_ok()
    );
}

#[test]
fn missing_and_broken_logos_fall_back_without_retrying_each_frame() {
    let root = directory();
    let server = Server::new(b"broken image".to_vec());
    let ctx = Context::default();
    let mut icons = ProjectIcons::new(root.path().into());
    for url in [None, Some(""), Some("https://example.com/logo.png")] {
        assert!(icons.get(&ctx, url).0.is_none());
    }
    assert_eq!(icons.active, 0);
    assert!(wait_for_icon(&mut icons, &ctx, &server.url).0.is_none());
    for _ in 0..20 {
        assert!(icons.get(&ctx, Some(&server.url)).0.is_none());
    }
    assert_eq!(icons.active, 0);
    assert_eq!(server.requests.load(Ordering::Relaxed), 1);
}

#[test]
fn logo_loading_limits_workers_and_evicts_old_textures() {
    let root = directory();
    let ctx = Context::default();
    let mut icons = ProjectIcons::new(root.path().into());
    let texture = ctx.load_texture(
        "old",
        ColorImage::filled([1, 1], egui::Color32::WHITE),
        TextureOptions::LINEAR,
    );
    for index in 0..MAX_TEXTURES {
        icons.entries.insert(
            format!("old-{index}"),
            Entry {
                state: State::Ready(texture.clone()),
                used: index as u64,
            },
        );
    }
    let server = Server::new(encoded(ImageFormat::Png, 16, 16));
    icons.get(&ctx, Some(&server.url));
    assert_eq!(icons.entries.len(), MAX_TEXTURES);
    assert!(!icons.entries.contains_key("old-0"));
    assert!(wait_for_icon(&mut icons, &ctx, &server.url).0.is_some());
    icons.active = MAX_WORKERS;
    icons.get(&ctx, Some(&format!("{}?different", server.url)));
    assert_eq!(icons.entries.len(), MAX_TEXTURES);
    assert_eq!(icons.active, MAX_WORKERS);
}

#[test]
#[ignore = "downloads real project logos from Modrinth; run explicitly"]
fn live_modrinth_mod_and_modpack_logos() {
    let root = directory();
    let client = Client::builder()
        .user_agent("JeffreyMing/NSCL logo integration test (https://cnb.cool/JeffreyMing/NorthStar-Minecraft-Launcher)")
        .timeout(Duration::from_secs(30))
        .build().unwrap();
    for (query, kind) in [("Sodium", "mod"), ("Fabulously Optimized", "modpack")] {
        let page: crate::content::SearchPage = client
            .get("https://api.modrinth.com/v2/search")
            .query(&[
                ("query", query),
                ("limit", "1"),
                ("facets", &format!("[[\"project_type:{kind}\"]]")),
            ])
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap();
        let project = page.hits.first().expect("no search result");
        let image = load(
            root.path(),
            project.icon_url.as_deref().expect("no project logo"),
        )
        .unwrap();
        assert!(image.size.iter().all(|size| (1..=128).contains(size)));
        println!(
            "{} ({kind}): {} × {}",
            project.title, image.size[0], image.size[1]
        );
    }
}
