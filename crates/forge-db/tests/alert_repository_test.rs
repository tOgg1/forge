use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::alert_repository::{Alert, AlertRepository, AlertSeverity, AlertType};
use forge_db::{Config, Db, DbError};

fn temp_db_path(prefix: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_nanos(),
        Err(_) => 0,
    };
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-alert-{prefix}-{nanos}-{}-{suffix}.sqlite",
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
fn create_list_resolve_roundtrip() {
    let (db, path) = setup_db("create-list-resolve");
    let repo = AlertRepository::new(&db);
    let (workspace_id, agent_id) = seed_workspace_and_agent(&db, "alert-1");

    let mut alert = Alert {
        workspace_id: workspace_id.clone(),
        agent_id: agent_id.clone(),
        alert_type: AlertType::ApprovalNeeded,
        severity: AlertSeverity::Warning,
        message: "approval required".to_string(),
        ..Default::default()
    };

    if let Err(err) = repo.create(&mut alert) {
        panic!("create alert failed: {err}");
    }

    assert!(!alert.id.is_empty());
    assert!(!alert.created_at.is_empty());
    assert!(!alert.is_resolved);

    let by_workspace = match repo.list_by_workspace(&workspace_id, false) {
        Ok(value) => value,
        Err(err) => panic!("list by workspace failed: {err}"),
    };
    assert_eq!(by_workspace.len(), 1);
    assert_eq!(by_workspace[0].id, alert.id);

    let by_agent = match repo.list_unresolved_by_agent(&agent_id) {
        Ok(value) => value,
        Err(err) => panic!("list unresolved by agent failed: {err}"),
    };
    assert_eq!(by_agent.len(), 1);

    if let Err(err) = repo.resolve(&alert.id) {
        panic!("resolve alert failed: {err}");
    }

    let unresolved = match repo.list_by_workspace(&workspace_id, false) {
        Ok(value) => value,
        Err(err) => panic!("list unresolved after resolve failed: {err}"),
    };
    assert!(unresolved.is_empty());

    let resolved = match repo.get(&alert.id) {
        Ok(value) => value,
        Err(err) => panic!("get alert failed: {err}"),
    };
    assert!(resolved.is_resolved);
    assert!(resolved.resolved_at.is_some());

    let _ = std::fs::remove_file(path);
}

#[test]
fn resolve_missing_returns_not_found() {
    let (db, path) = setup_db("resolve-missing");
    let repo = AlertRepository::new(&db);

    let result = repo.resolve("missing-id");
    assert!(
        matches!(result, Err(DbError::AlertNotFound)),
        "expected AlertNotFound, got: {result:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_requires_message() {
    let (db, path) = setup_db("requires-message");
    let repo = AlertRepository::new(&db);

    let mut alert = Alert {
        message: "   ".to_string(),
        ..Default::default()
    };

    let result = repo.create(&mut alert);
    assert!(matches!(result, Err(DbError::Validation(_))));

    let _ = std::fs::remove_file(path);
}
