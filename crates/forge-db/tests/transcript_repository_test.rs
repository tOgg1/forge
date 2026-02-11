use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::transcript_repository::{Transcript, TranscriptRepository};
use forge_db::{Config, Db, DbError};

fn temp_db_path(prefix: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_nanos(),
        Err(_) => 0,
    };
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-transcript-{prefix}-{nanos}-{}-{suffix}.sqlite",
        std::process::id(),
    ))
}

fn setup_db(prefix: &str) -> (Db, PathBuf) {
    let path = temp_db_path(prefix);
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_up() {
        panic!("migrate_up failed: {err}");
    }
    (db, path)
}

fn seed_agent(db: &Db, suffix: &str) -> String {
    let node_id = format!("node-{suffix}");
    let node_name = format!("local-{suffix}");
    let ws_id = format!("ws-{suffix}");
    let ws_name = format!("workspace-{suffix}");
    let session = format!("session-{suffix}");
    let agent_id = format!("agent-{suffix}");
    let pane = format!("pane-{suffix}");

    if let Err(err) = db.conn().execute(
        "INSERT INTO nodes (id, name, status, is_local, ssh_backend) VALUES (?1, ?2, 'online', 1, 'auto')",
        [node_id.as_str(), node_name.as_str()],
    ) {
        panic!("insert node failed: {err}");
    }

    if let Err(err) = db.conn().execute(
        "INSERT INTO workspaces (id, name, node_id, repo_path, tmux_session, status)
         VALUES (?1, ?2, ?3, '/tmp/repo', ?4, 'active')",
        [
            ws_id.as_str(),
            ws_name.as_str(),
            node_id.as_str(),
            session.as_str(),
        ],
    ) {
        panic!("insert workspace failed: {err}");
    }

    if let Err(err) = db.conn().execute(
        "INSERT INTO agents (
            id, workspace_id, type, tmux_pane, state, state_confidence
         ) VALUES (?1, ?2, 'opencode', ?3, 'idle', 'high')",
        [agent_id.as_str(), ws_id.as_str(), pane.as_str()],
    ) {
        panic!("insert agent failed: {err}");
    }

    agent_id
}

#[test]
fn create_list_latest_roundtrip() {
    let (db, path) = setup_db("create-list-latest");
    let repo = TranscriptRepository::new(&db);
    let agent_id = seed_agent(&db, "tx-1");

    let mut first = Transcript {
        agent_id: agent_id.clone(),
        content: "first".to_string(),
        content_hash: "hash-1".to_string(),
        captured_at: "2026-02-09T10:00:00Z".to_string(),
        ..Default::default()
    };
    if let Err(err) = repo.create(&mut first) {
        panic!("create first failed: {err}");
    }

    let mut second = Transcript {
        agent_id: agent_id.clone(),
        content: "second".to_string(),
        content_hash: "hash-2".to_string(),
        captured_at: "2026-02-09T10:01:00Z".to_string(),
        ..Default::default()
    };
    if let Err(err) = repo.create(&mut second) {
        panic!("create second failed: {err}");
    }

    let mut third = Transcript {
        agent_id: agent_id.clone(),
        content: "third".to_string(),
        content_hash: "hash-3".to_string(),
        captured_at: "2026-02-09T10:02:00Z".to_string(),
        ..Default::default()
    };
    if let Err(err) = repo.create(&mut third) {
        panic!("create third failed: {err}");
    }

    assert!(first.id > 0);
    assert!(second.id > 0);
    assert!(third.id > 0);

    let top_two = match repo.list_by_agent(&agent_id, 2) {
        Ok(value) => value,
        Err(err) => panic!("list_by_agent failed: {err}"),
    };
    assert_eq!(top_two.len(), 2);
    assert_eq!(top_two[0].content, "third");
    assert_eq!(top_two[1].content, "second");

    let latest = match repo.latest_by_agent(&agent_id) {
        Ok(value) => value,
        Err(err) => panic!("latest_by_agent failed: {err}"),
    };
    assert_eq!(latest.content, "third");
    assert_eq!(latest.content_hash, "hash-3");

    let fetched = match repo.get(first.id) {
        Ok(value) => value,
        Err(err) => panic!("get transcript failed: {err}"),
    };
    assert_eq!(fetched.content, "first");

    let _ = std::fs::remove_file(path);
}

#[test]
fn latest_missing_returns_not_found() {
    let (db, path) = setup_db("latest-missing");
    let repo = TranscriptRepository::new(&db);

    let result = repo.latest_by_agent("no-agent");
    assert!(
        matches!(result, Err(DbError::TranscriptNotFound)),
        "expected TranscriptNotFound, got: {result:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_requires_agent_id() {
    let (db, path) = setup_db("requires-agent");
    let repo = TranscriptRepository::new(&db);

    let mut transcript = Transcript {
        content: "hello".to_string(),
        content_hash: "hash".to_string(),
        ..Default::default()
    };

    let result = repo.create(&mut transcript);
    assert!(matches!(result, Err(DbError::Validation(_))));

    let _ = std::fs::remove_file(path);
}
