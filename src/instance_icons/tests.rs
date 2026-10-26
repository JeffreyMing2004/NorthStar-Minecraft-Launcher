use super::*;
use std::{
    io::{Cursor, Write},
    time::{Duration, Instant},
};

fn instance() -> Instance {
    Instance {
        id: "icon-test".into(),
        name: "Icon test".into(),
        version_id: "1.20.1".into(),
        ..Default::default()
    }
}
fn png(color: [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(12, 8, image::Rgba(color));
    let mut buffer = Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, image::ImageFormat::Png)
        .unwrap();
    buffer.into_inner()
}

#[test]
fn instance_icons_take_precedence_and_broken_images_fall_back_to_pcl() {
    let (root, _app, _ctx) = crate::service::tests::fixture();
    let instance = instance();
    let own = instance.directory(root.path()).unwrap();
    let pcl = root.path().join("versions/1.20.1/PCL");
    fs::create_dir_all(&own).unwrap();
    fs::create_dir_all(&pcl).unwrap();
    fs::write(own.join("icon.png"), png([250, 10, 20, 255])).unwrap();
    fs::write(pcl.join("Logo.png"), png([10, 240, 30, 255])).unwrap();
    let image = load(root.path(), &instance, None, Kind::Fabric).unwrap();
    assert_eq!(image.pixels[0], Color32::from_rgb(250, 10, 20));
    fs::write(own.join("icon.png"), b"broken").unwrap();
    let image = load(root.path(), &instance, None, Kind::Fabric).unwrap();
    assert_eq!(image.pixels[0], Color32::from_rgb(10, 240, 30));
    fs::write(own.join("icon.png"), vec![0u8; 4 * 1024 * 1024 + 1]).unwrap();
    assert!(load(root.path(), &instance, None, Kind::Fabric).is_some());
}

#[test]
fn vanilla_client_icons_are_read_without_extracting_files() {
    let (root, _app, _ctx) = crate::service::tests::fixture();
    let instance = instance();
    let version = root.path().join("versions/1.20.1");
    fs::create_dir_all(&version).unwrap();
    let mut jar = zip::ZipWriter::new(fs::File::create(version.join("1.20.1.jar")).unwrap());
    jar.start_file("pack.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    jar.write_all(&png([30, 40, 230, 255])).unwrap();
    jar.finish().unwrap();
    let icon = load(root.path(), &instance, None, Kind::Vanilla).unwrap();
    assert_eq!(icon.pixels[0], Color32::from_rgb(30, 40, 230));
    assert!(!version.join("pack.png").exists());
    assert!(load(root.path(), &instance, Some("../escape"), Kind::Pack).is_none());
}

#[test]
fn background_loads_are_deduplicated_and_refresh_reads_changed_icons() {
    let (root, _app, ctx) = crate::service::tests::fixture();
    let instance = instance();
    let own = instance.directory(root.path()).unwrap();
    fs::create_dir_all(&own).unwrap();
    fs::write(own.join("icon.png"), png([80, 90, 100, 255])).unwrap();
    let mut icons = InstanceIcons::default();
    for _ in 0..8 {
        icons.get(&ctx, root.path(), &instance, None, Kind::Pack);
    }
    assert_eq!(icons.entries.len(), 1);
    assert!(icons.active <= 1);
    let deadline = Instant::now() + Duration::from_secs(3);
    let texture = loop {
        if let Some(texture) = icons
            .get(&ctx, root.path(), &instance, None, Kind::Pack)
            .texture
        {
            break texture;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    fs::write(own.join("icon.png"), png([200, 190, 180, 255])).unwrap();
    icons.clear();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(updated) = icons
            .get(&ctx, root.path(), &instance, None, Kind::Pack)
            .texture
        {
            assert_ne!(updated.id(), texture.id());
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
