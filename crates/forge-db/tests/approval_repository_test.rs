use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::approval_repository::{Approval, ApprovalRepository, ApprovalStatus};
use forge_db::{Config, Db, DbError};

fn temp_db_path(prefix: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_nanos(),
        Err(_) => 0,
    };
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-approval-{prefix}-{nanos}-{}-{suffix}.sqlite",
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
    let ws_id = format!("ws-{suffix}");
    let agent_id = format!("agent-{suffix}");

    if let Err(err) = db.conn().execute(
        "INSERT INTO nodes (id, name, status, is_local, ssh_backend) VALUES (?1, ?2, 'online', 1, 'auto')",
        [node_id.as_str(), format!("local-{suffix}").as_str()],
    ) {
        panic!("insert node failed: {err}");
    }

    if let Err(err) = db.conn().execute(
        "INSERT INTO workspaces (id, name, node_id, repo_path, tmux_session, status)
         VALUES (?1, ?2, ?3, '/tmp/repo', ?4, 'active')",
        [
            ws_id.as_str(),
            format!("ws-{suffix}").as_str(),
            node_id.as_str(),
            format!("session-{suffix}").as_str(),
        ],
    ) {
        panic!("insert workspace failed: {err}");
    }

    if let Err(err) = db.conn().execute(
        "INSERT INTO agents (
            id, workspace_id, type, tmux_pane, state, state_confidence
         ) VALUES (?1, ?2, 'opencode', ?3, 'idle', 'high')",
        [
            agent_id.as_str(),
            ws_id.as_str(),
            format!("pane-{suffix}").as_str(),
        ],
    ) {
        panic!("insert agent failed: {err}");
    }

    agent_id
}

#[test]
fn create_list_update_parity() {
    let (db, path) = setup_db("create-list-update");
    let repo = ApprovalRepository::new(&db);
    let agent_id = seed_agent(&db, "approval-1");

    let mut approval = Approval {
        agent_id: agent_id.clone(),
        request_type: "file_write".to_string(),
        request_details_json: r#"{"path":"/tmp/test.txt"}"#.to_string(),
        ..Default::default()
    };

    if let Err(err) = repo.create(&mut approval) {
        panic!("create approval failed: {err}");
    }

    assert!(!approval.id.is_empty());
    assert!(!approval.created_at.is_empty());
    assert_eq!(approval.status, ApprovalStatus::Pending);

    let pending = match repo.list_pending_by_agent(&agent_id) {
        Ok(value) => value,
        Err(err) => panic!("list pending failed: {err}"),
    };
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, approval.id);

    if let Err(err) = repo.update_status(&approval.id, ApprovalStatus::Approved, "user") {
        panic!("update status failed: {err}");
    }

    let pending_after = match repo.list_pending_by_agent(&agent_id) {
        Ok(value) => value,
        Err(err) => panic!("list pending after update failed: {err}"),
    };
    assert!(pending_after.is_empty());

    let updated = match repo.get(&approval.id) {
        Ok(value) => value,
        Err(err) => panic!("get updated approval failed: {err}"),
    };
    assert_eq!(updated.status, ApprovalStatus::Approved);
    assert_eq!(updated.resolved_by, "user");
    assert!(updated.resolved_at.is_some());

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_status_missing_returns_not_found() {
    let (db, path) = setup_db("update-missing");
    let repo = ApprovalRepository::new(&db);

    let result = repo.update_status("missing", ApprovalStatus::Denied, "user");
    assert!(
        matches!(result, Err(DbError::ApprovalNotFound)),
        "expected ApprovalNotFound, got: {result:?}"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_validates_required_fields() {
    let (db, path) = setup_db("validate-fields");
    let repo = ApprovalRepository::new(&db);

    let mut approval = Approval {
        request_type: "file_write".to_string(),
        ..Default::default()
    };
    let result = repo.create(&mut approval);
    assert!(matches!(result, Err(DbError::Validation(_))));

    let mut approval = Approval {
        agent_id: "agent-x".to_string(),
        ..Default::default()
    };
    let result = repo.create(&mut approval);
    assert!(matches!(result, Err(DbError::Validation(_))));

    let _ = std::fs::remove_file(path);
}
