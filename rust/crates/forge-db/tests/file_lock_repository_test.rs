use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::file_lock_repository::FileLockRepository;
use forge_db::{Config, Db};
use rusqlite::OptionalExtension;

fn temp_db_path(prefix: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_nanos(),
        Err(_) => 0,
    };
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-file-lock-{prefix}-{nanos}-{}-{suffix}.sqlite",
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

fn seed_workspace_and_agent(db: &Db, suffix: &str) -> (String, String) {
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

    (ws_id, agent_id)
}

#[test]
fn cleanup_expired_marks_only_expired_active_locks() {
    let (db, path) = setup_db("cleanup-expired");
    let repo = FileLockRepository::new(&db);
    let (workspace_id, agent_id) = seed_workspace_and_agent(&db, "file-lock-1");

    let now = "2026-02-09T18:00:00Z";
    let expired = "2026-02-09T17:00:00Z";
    let active = "2026-02-09T19:00:00Z";
    let created_at = "2026-02-09T16:00:00Z";

    let insert = "INSERT INTO file_locks (
        id, workspace_id, agent_id, path_pattern, exclusive, reason,
        ttl_seconds, expires_at, created_at
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";

    if let Err(err) = db.conn().execute(
        insert,
        (
            "lock-expired",
            workspace_id.as_str(),
            agent_id.as_str(),
            "src/*.rs",
            1,
            "test",
            3600,
            expired,
            created_at,
        ),
    ) {
        panic!("insert expired lock failed: {err}");
    }

    if let Err(err) = db.conn().execute(
        insert,
        (
            "lock-active",
            workspace_id.as_str(),
            agent_id.as_str(),
            "README.md",
            1,
            "test",
            3600,
            active,
            created_at,
        ),
    ) {
        panic!("insert active lock failed: {err}");
    }

    let updated = match repo.cleanup_expired(Some(now)) {
        Ok(value) => value,
        Err(err) => panic!("cleanup_expired failed: {err}"),
    };
    assert_eq!(updated, 1);

    let expired_released: Option<String> = match db
        .conn()
        .query_row(
            "SELECT released_at FROM file_locks WHERE id = ?1",
            ["lock-expired"],
            |row| row.get(0),
        )
        .optional()
    {
        Ok(value) => value,
        Err(err) => panic!("query expired lock failed: {err}"),
    };
    assert!(expired_released.is_some());

    let active_released: Option<String> = match db
        .conn()
        .query_row(
            "SELECT released_at FROM file_locks WHERE id = ?1",
            ["lock-active"],
            |row| row.get(0),
        )
        .optional()
    {
        Ok(value) => value,
        Err(err) => panic!("query active lock failed: {err}"),
    };
    assert!(active_released.is_none());

    let _ = std::fs::remove_file(path);
}

#[test]
fn cleanup_expired_with_none_uses_current_time() {
    let (db, path) = setup_db("cleanup-none");
    let repo = FileLockRepository::new(&db);
    let (workspace_id, agent_id) = seed_workspace_and_agent(&db, "file-lock-2");

    let insert = "INSERT INTO file_locks (
        id, workspace_id, agent_id, path_pattern, exclusive, reason,
        ttl_seconds, expires_at, created_at
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";

    if let Err(err) = db.conn().execute(
        insert,
        (
            "lock-old",
            workspace_id.as_str(),
            agent_id.as_str(),
            "*.txt",
            1,
            "test",
            3600,
            "2000-01-01T00:00:00Z",
            "1999-12-31T23:59:00Z",
        ),
    ) {
        panic!("insert old lock failed: {err}");
    }

    let updated = match repo.cleanup_expired(None) {
        Ok(value) => value,
        Err(err) => panic!("cleanup_expired none failed: {err}"),
    };
    assert_eq!(updated, 1);

    let released: Option<String> = match db
        .conn()
        .query_row(
            "SELECT released_at FROM file_locks WHERE id = ?1",
            ["lock-old"],
            |row| row.get(0),
        )
        .optional()
    {
        Ok(value) => value,
        Err(err) => panic!("query old lock failed: {err}"),
    };
    assert!(released.is_some());

    let _ = std::fs::remove_file(path);
}
