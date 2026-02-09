#![allow(clippy::expect_used, clippy::unwrap_used)]

use chrono::{DateTime, TimeZone, Utc};
use fmail_core::store::Store;

fn fixed(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, hh, mm, ss).unwrap()
}

#[test]
fn ensure_project_creates_and_preserves_created_timestamp() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");

    let now1 = fixed(2026, 2, 9, 12, 0, 0);
    let proj1 = store
        .ensure_project("proj-123", now1)
        .expect("ensure project");
    assert_eq!(proj1.id, "proj-123");
    assert_eq!(proj1.created, now1);

    let read = store.read_project().expect("read").expect("some");
    assert_eq!(read.id, "proj-123");
    assert_eq!(read.created, now1);

    let now2 = fixed(2026, 2, 10, 12, 0, 0);
    let proj2 = store
        .ensure_project("proj-999", now2)
        .expect("ensure project again");
    // Existing file wins.
    assert_eq!(proj2.id, "proj-123");
    assert_eq!(proj2.created, now1);
}

#[test]
fn list_gc_files_scans_valid_topics_and_dms_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");
    store.ensure_root().expect("ensure root");

    let topics = store.root().join("topics");
    let dm = store.root().join("dm");

    std::fs::create_dir_all(topics.join("task")).expect("mkdir task");
    std::fs::create_dir_all(topics.join("Bad Topic!")).expect("mkdir bad topic");
    std::fs::create_dir_all(dm.join("alice")).expect("mkdir alice");
    std::fs::create_dir_all(dm.join("Bad Agent!")).expect("mkdir bad agent");

    let t1 = topics.join("task").join("20260101-120000-0001.json");
    let d1 = dm.join("alice").join("20260101-120001-0001.json");
    let bad1 = topics.join("Bad Topic!").join("20260101-120000-0001.json");
    let bad2 = dm.join("Bad Agent!").join("20260101-120000-0001.json");

    std::fs::write(&t1, b"{}").expect("write");
    std::fs::write(&d1, b"{}").expect("write");
    std::fs::write(&bad1, b"{}").expect("write");
    std::fs::write(&bad2, b"{}").expect("write");

    let mut files = store.list_gc_files().expect("list");
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let paths: Vec<String> = files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();
    assert_eq!(
        paths,
        vec![
            d1.to_string_lossy().to_string(),
            t1.to_string_lossy().to_string()
        ]
    );
}
