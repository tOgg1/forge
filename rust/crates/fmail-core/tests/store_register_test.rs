#![allow(clippy::expect_used, clippy::unwrap_used)]

use chrono::{TimeZone, Utc};
use fmail_core::store::{Store, ERR_AGENT_EXISTS};

#[test]
fn register_agent_record_unique() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");
    let fixed = Utc.with_ymd_and_hms(2026, 1, 10, 18, 0, 0).unwrap();

    let record = store
        .register_agent_record("Alice", "test-host", fixed)
        .expect("register alice");
    assert_eq!(record.name, "alice");
    assert_eq!(record.host.as_deref(), Some("test-host"));
    assert_eq!(record.first_seen, fixed);
    assert_eq!(record.last_seen, fixed);

    let err = store
        .register_agent_record("alice", "other-host", fixed)
        .expect_err("should fail on duplicate");
    assert_eq!(err, ERR_AGENT_EXISTS);
}

#[test]
fn register_then_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");
    let fixed = Utc.with_ymd_and_hms(2026, 1, 10, 18, 0, 0).unwrap();

    store
        .register_agent_record("beta", "h1", fixed)
        .expect("register beta");
    store
        .register_agent_record("alpha", "h2", fixed)
        .expect("register alpha");

    let records = store.list_agent_records().expect("list").expect("some");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].name, "alpha");
    assert_eq!(records[1].name, "beta");
}

#[test]
fn register_empty_host() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");
    let fixed = Utc.with_ymd_and_hms(2026, 1, 10, 18, 0, 0).unwrap();

    let record = store
        .register_agent_record("agent-1", "", fixed)
        .expect("register");
    assert!(record.host.is_none(), "host should be None for empty");
}

#[test]
fn register_invalid_name_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");
    let fixed = Utc.with_ymd_and_hms(2026, 1, 10, 18, 0, 0).unwrap();

    let err = store
        .register_agent_record("bad name!", "host", fixed)
        .expect_err("should fail");
    assert!(
        err.contains("invalid agent name"),
        "unexpected error: {err}"
    );
}
