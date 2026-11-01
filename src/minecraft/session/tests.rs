use super::*;
fn fixture() -> tempfile::TempDir {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-work");
    fs::create_dir_all(&p).unwrap();
    tempfile::tempdir_in(p).unwrap()
}
#[test]
fn stale_files_are_excluded_new_append_and_crash_are_captured_and_frozen() {
    let root = fixture();
    fs::create_dir_all(root.path().join("logs")).unwrap();
    fs::create_dir_all(root.path().join("crash-reports")).unwrap();
    fs::write(
        root.path().join("logs/latest.log"),
        "OLD OutOfMemoryError\n",
    )
    .unwrap();
    fs::write(
        root.path().join("crash-reports/crash-old.txt"),
        "OLD Mixin apply failed",
    )
    .unwrap();
    let session = LaunchSession::begin(root.path()).unwrap();
    fs::write(session.log_path(), "new launch").unwrap();
    let logs = LaunchSession::logs(&session.log_path()).unwrap().unwrap();
    assert_eq!(logs.len(), 1);
    fs::write(
        root.path().join("logs/latest.log"),
        "OLD OutOfMemoryError\nNEW Incompatible mods found\n",
    )
    .unwrap();
    fs::write(
        root.path().join("hs_err_pid123.log"),
        "EXCEPTION_ACCESS_VIOLATION new",
    )
    .unwrap();
    let logs = LaunchSession::logs(&session.log_path()).unwrap().unwrap();
    assert!(logs.iter().any(|l| l.text.contains("NEW Incompatible")));
    assert!(logs.iter().any(|l| matches!(l.kind, LogKind::Jvm)));
    assert!(!logs.iter().any(|l| l.text.contains("OLD")));
    session.finish().unwrap();
    fs::write(root.path().join("logs/latest.log"), "ANOTHER RUN").unwrap();
    let logs = LaunchSession::logs(&session.log_path()).unwrap().unwrap();
    assert!(!logs.iter().any(|l| l.text.contains("ANOTHER RUN")));
    assert!(logs.iter().any(|l| l.text.contains("NEW Incompatible")));
}
#[test]
fn rewritten_logs_and_independent_sessions_do_not_mix() {
    let root = fixture();
    fs::create_dir_all(root.path().join("logs")).unwrap();
    fs::write(
        root.path().join("logs/latest.log"),
        "long old previous errors",
    )
    .unwrap();
    let first = LaunchSession::begin(root.path()).unwrap();
    fs::write(first.log_path(), "first").unwrap();
    fs::write(root.path().join("logs/latest.log"), "short new").unwrap();
    first.finish().unwrap();
    let second = LaunchSession::begin(root.path()).unwrap();
    fs::write(second.log_path(), "second").unwrap();
    assert_ne!(first.log_path(), second.log_path());
    assert_eq!(
        LaunchSession::logs(&second.log_path())
            .unwrap()
            .unwrap()
            .len(),
        1
    );
    assert!(
        LaunchSession::logs(&first.log_path())
            .unwrap()
            .unwrap()
            .iter()
            .any(|l| l.text == "short new")
    );
}
