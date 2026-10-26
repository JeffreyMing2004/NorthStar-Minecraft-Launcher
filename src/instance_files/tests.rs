use super::*;
use std::time::{Duration, Instant};

fn temporary() -> tempfile::TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&parent).unwrap();
    tempfile::tempdir_in(parent).unwrap()
}

#[test]
fn reading_missing_directories_does_not_create_them_and_entries_are_bounded() {
    let root = temporary();
    let missing = root.path().join("screenshots");
    let listing = read_directory(&missing).unwrap();
    assert!(!listing.exists);
    assert!(!missing.exists());
    fs::create_dir(root.path().join("world")).unwrap();
    fs::write(root.path().join("example.jar"), [0; 1200]).unwrap();
    let listing = read_directory(root.path()).unwrap();
    assert_eq!(listing.entries[0].name, "world");
    assert!(listing.entries[0].directory);
    assert_eq!(listing.entries[1].size, 1200);
    for index in 0..MAX_ENTRIES {
        fs::write(root.path().join(format!("{index}.txt")), []).unwrap();
    }
    let listing = read_directory(root.path()).unwrap();
    assert!(listing.truncated);
    assert_eq!(listing.entries.len(), MAX_ENTRIES);
}

fn wait(browser: &mut Browser, ctx: &egui::Context, path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        browser.request(ctx, path);
        if browser
            .data
            .as_ref()
            .is_some_and(|(loaded, _)| loaded == path)
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn switching_directories_discards_old_results_and_refresh_reloads_files() {
    let root = temporary();
    let mods = root.path().join("mods");
    let logs = root.path().join("logs");
    fs::create_dir(&mods).unwrap();
    fs::create_dir(&logs).unwrap();
    fs::write(mods.join("example.jar"), b"jar").unwrap();
    let ctx = egui::Context::default();
    let mut browser = Browser::default();
    browser.request(&ctx, &mods);
    browser.select(Folder::Logs, None);
    wait(&mut browser, &ctx, &logs);
    assert!(
        browser
            .data
            .as_ref()
            .unwrap()
            .1
            .as_ref()
            .unwrap()
            .entries
            .is_empty()
    );
    fs::write(logs.join("latest.log"), b"log").unwrap();
    browser.data = None;
    wait(&mut browser, &ctx, &logs);
    assert_eq!(
        browser.data.as_ref().unwrap().1.as_ref().unwrap().entries[0].name,
        "latest.log"
    );
    let bad = mods.join("example.jar");
    wait(&mut browser, &ctx, &bad);
    assert!(browser.data.as_ref().unwrap().1.is_err());
}
