#![allow(clippy::expect_used, clippy::unwrap_used)]

use chrono::{TimeZone, Utc};
use fmail_core::store::Store;

#[test]
fn read_missing_agent_record_is_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");
    let record = store.read_agent_record("alice").expect("read");
    assert!(record.is_none());
}

#[test]
fn set_status_creates_record_and_trims() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");
    let now = Utc.with_ymd_and_hms(2026, 2, 9, 12, 0, 0).unwrap();

    let record = store
        .set_agent_status("Alice", "  working on auth  ", "test-host", now)
        .expect("set status");

    assert_eq!(record.name, "alice");
    assert_eq!(record.host.as_deref(), Some("test-host"));
    assert_eq!(record.status.as_deref(), Some("working on auth"));
    assert_eq!(record.first_seen, now);
    assert_eq!(record.last_seen, now);

    let loaded = store
        .read_agent_record("alice")
        .expect("read")
        .expect("some");
    assert_eq!(loaded.status.as_deref(), Some("working on auth"));
}

#[test]
fn clear_status_sets_none_and_preserves_host_when_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");
    let t1 = Utc.with_ymd_and_hms(2026, 2, 9, 12, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2026, 2, 9, 12, 1, 0).unwrap();

    store
        .set_agent_status("alice", "working", "h1", t1)
        .expect("set");

    let cleared = store
        .set_agent_status("alice", "   ", "", t2)
        .expect("clear");
    assert!(cleared.status.is_none());
    assert_eq!(cleared.host.as_deref(), Some("h1"));
    assert_eq!(cleared.first_seen, t1);
    assert_eq!(cleared.last_seen, t2);
}
