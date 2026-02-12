#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::persistent_agent_event_repository::{
    PersistentAgentEvent, PersistentAgentEventQuery, PersistentAgentEventRepository,
};
use forge_db::{Config, Db};

fn open_db(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let mut db = Db::open(Config::new(&path)).expect("open db");
    db.migrate_up().expect("migrate_up");
    (db, path)
}

#[test]
fn append_and_get() {
    let (db, path) = open_db("pae-append-get");
    let repo = PersistentAgentEventRepository::new(&db);

    let mut event = PersistentAgentEvent {
        id: 0,
        agent_id: Some("agent-1".into()),
        kind: "spawn".into(),
        outcome: "success".into(),
        detail: Some("spawned agent".into()),
        timestamp: String::new(),
    };

    repo.append(&mut event).expect("append");
    assert!(event.id > 0, "id should be auto-assigned");
    assert!(!event.timestamp.is_empty(), "timestamp should be set");

    let fetched = repo.get(event.id).expect("get");
    assert_eq!(fetched.agent_id, Some("agent-1".into()));
    assert_eq!(fetched.kind, "spawn");
    assert_eq!(fetched.outcome, "success");
    assert_eq!(fetched.detail, Some("spawned agent".into()));

    let _ = std::fs::remove_file(path);
}

#[test]
fn append_validation() {
    let (db, path) = open_db("pae-validation");
    let repo = PersistentAgentEventRepository::new(&db);

    // Missing kind.
    let mut event = PersistentAgentEvent {
        id: 0,
        agent_id: Some("agent-1".into()),
        kind: String::new(),
        outcome: "success".into(),
        detail: None,
        timestamp: String::new(),
    };
    assert!(repo.append(&mut event).is_err(), "should reject empty kind");

    // Missing outcome.
    let mut event = PersistentAgentEvent {
        id: 0,
        agent_id: Some("agent-1".into()),
        kind: "spawn".into(),
        outcome: String::new(),
        detail: None,
        timestamp: String::new(),
    };
    assert!(
        repo.append(&mut event).is_err(),
        "should reject empty outcome"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn append_without_agent_id() {
    let (db, path) = open_db("pae-no-agent");
    let repo = PersistentAgentEventRepository::new(&db);

    let mut event = PersistentAgentEvent {
        id: 0,
        agent_id: None,
        kind: "list_agents".into(),
        outcome: "success".into(),
        detail: None,
        timestamp: String::new(),
    };

    repo.append(&mut event).expect("append without agent_id");
    let fetched = repo.get(event.id).expect("get");
    assert_eq!(fetched.agent_id, None);

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_by_agent() {
    let (db, path) = open_db("pae-list-agent");
    let repo = PersistentAgentEventRepository::new(&db);

    for kind in &["spawn", "send_message", "wait_state"] {
        let mut event = PersistentAgentEvent {
            id: 0,
            agent_id: Some("agent-1".into()),
            kind: kind.to_string(),
            outcome: "success".into(),
            detail: None,
            timestamp: String::new(),
        };
        repo.append(&mut event).expect("append");
    }

    // Different agent.
    let mut other = PersistentAgentEvent {
        id: 0,
        agent_id: Some("agent-2".into()),
        kind: "spawn".into(),
        outcome: "success".into(),
        detail: None,
        timestamp: String::new(),
    };
    repo.append(&mut other).expect("append other");

    let events = repo.list_by_agent("agent-1", 100).expect("list_by_agent");
    assert_eq!(events.len(), 3);

    let events_2 = repo.list_by_agent("agent-2", 100).expect("list_by_agent");
    assert_eq!(events_2.len(), 1);

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_by_agent_limit() {
    let (db, path) = open_db("pae-list-limit");
    let repo = PersistentAgentEventRepository::new(&db);

    for i in 0..5 {
        let mut event = PersistentAgentEvent {
            id: 0,
            agent_id: Some("agent-1".into()),
            kind: format!("op-{i}"),
            outcome: "success".into(),
            detail: None,
            timestamp: String::new(),
        };
        repo.append(&mut event).expect("append");
    }

    let limited = repo.list_by_agent("agent-1", 2).expect("list limit");
    assert_eq!(limited.len(), 2);

    let _ = std::fs::remove_file(path);
}

#[test]
fn query_by_kind() {
    let (db, path) = open_db("pae-query-kind");
    let repo = PersistentAgentEventRepository::new(&db);

    for (kind, outcome) in &[
        ("spawn", "success"),
        ("spawn", "error: failed"),
        ("kill", "success"),
    ] {
        let mut event = PersistentAgentEvent {
            id: 0,
            agent_id: Some("agent-1".into()),
            kind: kind.to_string(),
            outcome: outcome.to_string(),
            detail: None,
            timestamp: String::new(),
        };
        repo.append(&mut event).expect("append");
    }

    let spawns = repo
        .query(PersistentAgentEventQuery {
            kind: Some("spawn".into()),
            ..Default::default()
        })
        .expect("query spawns");
    assert_eq!(spawns.len(), 2);

    let kills = repo
        .query(PersistentAgentEventQuery {
            kind: Some("kill".into()),
            ..Default::default()
        })
        .expect("query kills");
    assert_eq!(kills.len(), 1);

    let _ = std::fs::remove_file(path);
}

#[test]
fn count_and_count_by_agent() {
    let (db, path) = open_db("pae-count");
    let repo = PersistentAgentEventRepository::new(&db);

    assert_eq!(repo.count().expect("count empty"), 0);

    for agent in &["agent-1", "agent-1", "agent-2"] {
        let mut event = PersistentAgentEvent {
            id: 0,
            agent_id: Some(agent.to_string()),
            kind: "spawn".into(),
            outcome: "success".into(),
            detail: None,
            timestamp: String::new(),
        };
        repo.append(&mut event).expect("append");
    }

    assert_eq!(repo.count().expect("count total"), 3);
    assert_eq!(repo.count_by_agent("agent-1").expect("count agent-1"), 2);
    assert_eq!(repo.count_by_agent("agent-2").expect("count agent-2"), 1);
    assert_eq!(
        repo.count_by_agent("nonexistent").expect("count missing"),
        0
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_not_found() {
    let (db, path) = open_db("pae-get-notfound");
    let repo = PersistentAgentEventRepository::new(&db);

    let err = repo.get(999);
    assert!(err.is_err(), "should error on missing event");

    let _ = std::fs::remove_file(path);
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_nanos(),
        Err(err) => panic!("clock before epoch: {err}"),
    };
    let suffix = uuid::Uuid::new_v4();
    std::env::temp_dir().join(format!("forge-db-{prefix}-{nanos}-{suffix}.sqlite"))
}
