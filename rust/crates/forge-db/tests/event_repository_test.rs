use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::event_repository::{Event, EventQuery, EventRepository};
use forge_db::{Config, Db, DbError};

fn temp_db_path(tag: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    std::env::temp_dir().join(format!(
        "forge-db-event-repo-{tag}-{nanos}-{}.sqlite",
        std::process::id()
    ))
}

fn open_migrated(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let mut db = match Db::open(Config::new(&path)) {
        Ok(db) => db,
        Err(err) => panic!("open db: {err}"),
    };
    match db.migrate_up() {
        Ok(_) => {}
        Err(err) => panic!("migrate_up: {err}"),
    }
    (db, path)
}

#[test]
fn append_and_query_roundtrip() {
    let (db, path) = open_migrated("append-query");
    let repo = EventRepository::new(&db);

    let mut event = Event {
        event_type: "node.online".to_string(),
        entity_type: "node".to_string(),
        entity_id: "node-1".to_string(),
        timestamp: "2026-01-10T10:00:00Z".to_string(),
        payload: "{\"status\":\"online\"}".to_string(),
        metadata: Some(std::collections::HashMap::from([(
            "source".to_string(),
            "test".to_string(),
        )])),
        ..Event::default()
    };

    if let Err(err) = repo.append(&mut event) {
        panic!("append: {err}");
    }
    assert!(!event.id.is_empty(), "append should set id");

    let page = match repo.query(EventQuery {
        event_type: Some("node.online".to_string()),
        limit: 10,
        ..EventQuery::default()
    }) {
        Ok(page) => page,
        Err(err) => panic!("query: {err}"),
    };
    assert_eq!(page.events.len(), 1);
    let got = &page.events[0];
    assert_eq!(got.event_type, "node.online");
    assert_eq!(got.entity_id, "node-1");
    assert_eq!(got.payload, "{\"status\":\"online\"}");
    match &got.metadata {
        Some(metadata) => assert_eq!(metadata.get("source"), Some(&"test".to_string())),
        None => panic!("metadata should exist"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn cursor_pagination_matches_go_shape() {
    let (db, path) = open_migrated("cursor");
    let repo = EventRepository::new(&db);

    for i in 0..3 {
        let mut event = Event {
            event_type: "agent.spawned".to_string(),
            entity_type: "agent".to_string(),
            entity_id: "agent-1".to_string(),
            timestamp: format!("2026-01-10T10:00:0{}Z", i),
            ..Event::default()
        };
        if let Err(err) = repo.append(&mut event) {
            panic!("append event {i}: {err}");
        }
    }

    let page1 = match repo.query(EventQuery {
        limit: 2,
        ..EventQuery::default()
    }) {
        Ok(page) => page,
        Err(err) => panic!("query page1: {err}"),
    };
    assert_eq!(page1.events.len(), 2);
    assert!(
        !page1.next_cursor.is_empty(),
        "next_cursor should be set for first page"
    );

    let page2 = match repo.query(EventQuery {
        cursor: page1.next_cursor.clone(),
        limit: 2,
        ..EventQuery::default()
    }) {
        Ok(page) => page,
        Err(err) => panic!("query page2: {err}"),
    };
    assert_eq!(page2.events.len(), 1);

    let _ = std::fs::remove_file(path);
}

#[test]
fn time_range_and_entity_list_filters() {
    let (db, path) = open_migrated("time-range");
    let repo = EventRepository::new(&db);

    let mut first = Event {
        event_type: "workspace.created".to_string(),
        entity_type: "workspace".to_string(),
        entity_id: "ws-1".to_string(),
        timestamp: "2026-01-10T10:00:00Z".to_string(),
        ..Event::default()
    };
    let mut second = Event {
        event_type: "workspace.created".to_string(),
        entity_type: "workspace".to_string(),
        entity_id: "ws-2".to_string(),
        timestamp: "2026-01-10T10:00:05Z".to_string(),
        ..Event::default()
    };
    if let Err(err) = repo.append(&mut first) {
        panic!("append first: {err}");
    }
    if let Err(err) = repo.append(&mut second) {
        panic!("append second: {err}");
    }

    let since_page = match repo.query(EventQuery {
        since: Some("2026-01-10T10:00:03Z".to_string()),
        ..EventQuery::default()
    }) {
        Ok(page) => page,
        Err(err) => panic!("query since: {err}"),
    };
    assert_eq!(since_page.events.len(), 1);
    assert_eq!(since_page.events[0].entity_id, "ws-2");

    let until_page = match repo.query(EventQuery {
        until: Some("2026-01-10T10:00:02Z".to_string()),
        ..EventQuery::default()
    }) {
        Ok(page) => page,
        Err(err) => panic!("query until: {err}"),
    };
    assert_eq!(until_page.events.len(), 1);
    assert_eq!(until_page.events[0].entity_id, "ws-1");

    let listed = match repo.list_by_entity("workspace", "ws-2", 10) {
        Ok(events) => events,
        Err(err) => panic!("list_by_entity: {err}"),
    };
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].entity_id, "ws-2");

    let _ = std::fs::remove_file(path);
}

#[test]
fn maintenance_operations_behave() {
    let (db, path) = open_migrated("maintenance");
    let repo = EventRepository::new(&db);

    let mut ids = Vec::new();
    for i in 0..4 {
        let mut event = Event {
            event_type: "agent.state_changed".to_string(),
            entity_type: "agent".to_string(),
            entity_id: format!("agent-{i}"),
            timestamp: format!("2026-01-10T10:00:0{}Z", i),
            ..Event::default()
        };
        if let Err(err) = repo.append(&mut event) {
            panic!("append {i}: {err}");
        }
        ids.push(event.id.clone());
    }

    let total = match repo.count() {
        Ok(value) => value,
        Err(err) => panic!("count: {err}"),
    };
    assert_eq!(total, 4);

    let oldest = match repo.oldest_timestamp() {
        Ok(value) => value,
        Err(err) => panic!("oldest_timestamp: {err}"),
    };
    assert_eq!(oldest, Some("2026-01-10T10:00:00Z".to_string()));

    let listed_old = match repo.list_older_than("2026-01-10T10:00:02Z", 10) {
        Ok(events) => events,
        Err(err) => panic!("list_older_than: {err}"),
    };
    assert_eq!(listed_old.len(), 2);

    let listed_oldest = match repo.list_oldest(2) {
        Ok(events) => events,
        Err(err) => panic!("list_oldest: {err}"),
    };
    assert_eq!(listed_oldest.len(), 2);

    let deleted_older = match repo.delete_older_than("2026-01-10T10:00:01Z", 100) {
        Ok(value) => value,
        Err(err) => panic!("delete_older_than: {err}"),
    };
    assert_eq!(deleted_older, 1);

    let deleted_excess = match repo.delete_excess(2, 100) {
        Ok(value) => value,
        Err(err) => panic!("delete_excess: {err}"),
    };
    assert_eq!(deleted_excess, 1);

    let remaining = match repo.count() {
        Ok(value) => value,
        Err(err) => panic!("count after delete_excess: {err}"),
    };
    assert_eq!(remaining, 2);

    let deleted_by_ids = match repo.delete_by_ids(&[ids[2].clone(), ids[3].clone()]) {
        Ok(value) => value,
        Err(err) => panic!("delete_by_ids: {err}"),
    };
    assert!(
        deleted_by_ids >= 1,
        "delete_by_ids should remove remaining ids"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn append_validation_matches_go() {
    let (db, path) = open_migrated("validation");
    let repo = EventRepository::new(&db);

    let mut invalid = Event::default();
    let err = repo.append(&mut invalid);
    assert!(
        matches!(err, Err(DbError::InvalidEvent)),
        "expected InvalidEvent, got {err:?}"
    );

    let _ = std::fs::remove_file(path);
}
