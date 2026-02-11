//! File lock repository integration tests — Go parity coverage.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::file_lock_repository::FileLockRepository;
use forge_db::{Config, Db};
use rusqlite::params;

fn temp_db_path(prefix: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-file-lock-{prefix}-{nanos}-{}-{suffix}.sqlite",
        std::process::id(),
    ))
}

fn setup_db(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let mut db = match Db::open(Config::new(&path)) {
        Ok(db) => db,
        Err(err) => panic!("open db: {err}"),
    };
    if let Err(err) = db.migrate_up() {
        panic!("migrate_up: {err}");
    }
    (db, path)
}

fn seed_workspace_and_agent(db: &Db) -> (String, String) {
    let node_id = "node-1".to_string();
    let ws_id = "ws-1".to_string();
    let agent_id = "agent-1".to_string();

    let conn = db.conn();
    conn.execute(
        "INSERT INTO nodes (id, name) VALUES (?1, ?2)",
        params![node_id, "node-1"],
    )
    .unwrap_or_else(|e| panic!("insert node: {e}"));

    conn.execute(
        "INSERT INTO workspaces (id, name, node_id, repo_path, tmux_session)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![ws_id, "ws", "node-1", "/tmp/repo", "forge-test:0"],
    )
    .unwrap_or_else(|e| panic!("insert workspace: {e}"));

    conn.execute(
        "INSERT INTO agents (id, workspace_id, type, tmux_pane)
         VALUES (?1, ?2, ?3, ?4)",
        params![agent_id, ws_id, "opencode", "forge-test:0.1"],
    )
    .unwrap_or_else(|e| panic!("insert agent: {e}"));

    (ws_id, agent_id)
}

#[test]
fn cleanup_expired_marks_only_expired() {
    let (db, path) = setup_db("cleanup-expired");
    let (ws_id, agent_id) = seed_workspace_and_agent(&db);

    let now = "2026-02-09T18:00:00Z";
    let expired = "2026-02-09T17:00:00Z";
    let active = "2026-02-09T19:00:00Z";
    let created_at = "2026-02-09T16:00:00Z";

    let conn = db.conn();
    let insert = "INSERT INTO file_locks (
            id, workspace_id, agent_id, path_pattern, exclusive, reason,
            ttl_seconds, expires_at, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";

    conn.execute(
        insert,
        params![
            "lock-expired",
            ws_id,
            agent_id,
            "src/*.go",
            1i64,
            "test",
            3600i64,
            expired,
            created_at
        ],
    )
    .unwrap_or_else(|e| panic!("insert expired lock: {e}"));

    conn.execute(
        insert,
        params![
            "lock-active",
            ws_id,
            agent_id,
            "README.md",
            1i64,
            "test",
            3600i64,
            active,
            created_at
        ],
    )
    .unwrap_or_else(|e| panic!("insert active lock: {e}"));

    let repo = FileLockRepository::new(&db);
    let updated = repo
        .cleanup_expired(Some(now))
        .unwrap_or_else(|e| panic!("cleanup_expired: {e}"));
    assert_eq!(updated, 1);

    let expired_released: Option<String> = conn
        .query_row(
            "SELECT released_at FROM file_locks WHERE id = ?1",
            params!["lock-expired"],
            |row| row.get::<_, Option<String>>(0),
        )
        .unwrap_or_else(|e| panic!("query expired lock: {e}"));
    assert!(
        expired_released.is_some(),
        "expired lock should be released"
    );

    let active_released: Option<String> = conn
        .query_row(
            "SELECT released_at FROM file_locks WHERE id = ?1",
            params!["lock-active"],
            |row| row.get::<_, Option<String>>(0),
        )
        .unwrap_or_else(|e| panic!("query active lock: {e}"));
    assert!(
        active_released.is_none(),
        "active lock should remain unreleased"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn cleanup_expired_uses_now_when_missing() {
    let (db, path) = setup_db("cleanup-expired-none");
    let (ws_id, agent_id) = seed_workspace_and_agent(&db);

    db.conn()
        .execute(
            "INSERT INTO file_locks (
                id, workspace_id, agent_id, path_pattern, exclusive, reason,
                ttl_seconds, expires_at, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                "lock-expired",
                ws_id,
                agent_id,
                "src/*.go",
                1i64,
                "test",
                3600i64,
                "1970-01-01T00:00:00Z",
                "1970-01-01T00:00:00Z",
            ],
        )
        .unwrap_or_else(|e| panic!("insert lock: {e}"));

    let repo = FileLockRepository::new(&db);
    let updated = repo
        .cleanup_expired(None)
        .unwrap_or_else(|e| panic!("cleanup_expired: {e}"));
    assert_eq!(updated, 1);

    let released_at: Option<String> = db
        .conn()
        .query_row(
            "SELECT released_at FROM file_locks WHERE id = ?1",
            params!["lock-expired"],
            |row| row.get::<_, Option<String>>(0),
        )
        .unwrap_or_else(|e| panic!("query lock: {e}"));
    assert!(released_at.is_some());

    let _ = std::fs::remove_file(path);
}
