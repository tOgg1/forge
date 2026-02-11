#![allow(clippy::expect_used, clippy::unwrap_used)]

use chrono::{DateTime, TimeZone, Utc};
use fmail_core::message::Message;
use fmail_core::store::{parse_message_time, Store};
use serde_json::json;

fn fixed(ts: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(ts)
        .expect("parse")
        .with_timezone(&Utc)
}

fn empty_message(from: &str, to: &str) -> Message {
    Message {
        id: String::new(),
        from: from.to_string(),
        to: to.to_string(),
        time: DateTime::<Utc>::default(),
        body: json!("hello"),
        reply_to: String::new(),
        priority: String::new(),
        host: String::new(),
        tags: vec![],
    }
}

#[test]
fn save_topic_message_round_trips_and_list_topics_tracks_activity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");
    let now = Utc.with_ymd_and_hms(2026, 2, 9, 12, 0, 0).unwrap();

    let mut msg = empty_message("Alice", "task");
    let id = store.save_message(&mut msg, now).expect("save");
    assert_eq!(msg.id, id);
    assert_eq!(msg.from, "alice");
    assert_eq!(msg.to, "task");
    assert_eq!(msg.time, now);

    let path = store.topic_message_path("task", &id);
    assert!(path.exists(), "expected message file at {}", path.display());

    let read = store.read_message(&path).expect("read");
    assert_eq!(read.id, id);
    assert_eq!(read.from, "alice");
    assert_eq!(read.to, "task");
    assert_eq!(read.time, now);
    assert_eq!(read.body, json!("hello"));

    let topics = store.list_topics().expect("list topics");
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0].name, "task");
    assert_eq!(topics[0].messages, 1);
    assert_eq!(topics[0].last_activity, parse_message_time(&id));
}

#[test]
fn save_dm_message_writes_to_dm_dir_and_lists_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("new store");
    let now = fixed("2026-02-09T12:00:00Z");

    let mut dm = empty_message("alice", "@Bob");
    let id = store.save_message(&mut dm, now).expect("save dm");
    assert_eq!(dm.to, "@bob");

    let dm_path = store.dm_message_path("bob", &id);
    assert!(
        dm_path.exists(),
        "expected dm file at {}",
        dm_path.display()
    );

    let files = store.list_dm_message_files("bob").expect("list dm files");
    assert_eq!(files, vec![dm_path.clone()]);

    let all = store.list_all_message_files().expect("list all");
    assert_eq!(all, vec![dm_path]);
}
