use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_001_up_down_parity() {
    let path = temp_db_path("migration-001");

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(1) {
        panic!("migrate_to(1) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    // Tables
    assert!(table_exists(&conn, "nodes"));
    assert!(table_exists(&conn, "workspaces"));
    assert!(table_exists(&conn, "accounts"));
    assert!(table_exists(&conn, "agents"));
    assert!(table_exists(&conn, "queue_items"));
    assert!(table_exists(&conn, "events"));
    assert!(table_exists(&conn, "alerts"));
    assert!(table_exists(&conn, "transcripts"));
    assert!(table_exists(&conn, "approvals"));

    // Indexes: nodes
    assert!(index_exists(&conn, "idx_nodes_name"));
    assert!(index_exists(&conn, "idx_nodes_status"));

    // Indexes: workspaces
    assert!(index_exists(&conn, "idx_workspaces_node_id"));
    assert!(index_exists(&conn, "idx_workspaces_status"));
    assert!(index_exists(&conn, "idx_workspaces_name"));

    // Indexes: accounts
    assert!(index_exists(&conn, "idx_accounts_provider"));
    assert!(index_exists(&conn, "idx_accounts_is_active"));
    assert!(index_exists(&conn, "idx_accounts_cooldown"));

    // Indexes: agents
    assert!(index_exists(&conn, "idx_agents_workspace_id"));
    assert!(index_exists(&conn, "idx_agents_state"));
    assert!(index_exists(&conn, "idx_agents_account_id"));
    assert!(index_exists(&conn, "idx_agents_type"));

    // Indexes: queue_items
    assert!(index_exists(&conn, "idx_queue_items_agent_id"));
    assert!(index_exists(&conn, "idx_queue_items_status"));
    assert!(index_exists(&conn, "idx_queue_items_position"));

    // Indexes: events
    assert!(index_exists(&conn, "idx_events_timestamp"));
    assert!(index_exists(&conn, "idx_events_type"));
    assert!(index_exists(&conn, "idx_events_entity"));
    assert!(index_exists(&conn, "idx_events_entity_timestamp"));

    // Indexes: alerts
    assert!(index_exists(&conn, "idx_alerts_workspace_id"));
    assert!(index_exists(&conn, "idx_alerts_agent_id"));
    assert!(index_exists(&conn, "idx_alerts_is_resolved"));
    assert!(index_exists(&conn, "idx_alerts_severity"));

    // Indexes: transcripts
    assert!(index_exists(&conn, "idx_transcripts_agent_id"));
    assert!(index_exists(&conn, "idx_transcripts_captured_at"));
    assert!(index_exists(&conn, "idx_transcripts_hash"));

    // Indexes: approvals
    assert!(index_exists(&conn, "idx_approvals_agent_id"));
    assert!(index_exists(&conn, "idx_approvals_status"));

    // Triggers
    assert!(trigger_exists(&conn, "update_nodes_timestamp"));
    assert!(trigger_exists(&conn, "update_workspaces_timestamp"));
    assert!(trigger_exists(&conn, "update_agents_timestamp"));
    assert!(trigger_exists(&conn, "update_accounts_timestamp"));

    drop(conn);

    // Down migration: back to version 0
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(0) {
        panic!("migrate_to(0) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    // Tables removed
    assert!(!table_exists(&conn, "nodes"));
    assert!(!table_exists(&conn, "workspaces"));
    assert!(!table_exists(&conn, "accounts"));
    assert!(!table_exists(&conn, "agents"));
    assert!(!table_exists(&conn, "queue_items"));
    assert!(!table_exists(&conn, "events"));
    assert!(!table_exists(&conn, "alerts"));
    assert!(!table_exists(&conn, "transcripts"));
    assert!(!table_exists(&conn, "approvals"));

    // Indexes removed: nodes
    assert!(!index_exists(&conn, "idx_nodes_name"));
    assert!(!index_exists(&conn, "idx_nodes_status"));

    // Indexes removed: workspaces
    assert!(!index_exists(&conn, "idx_workspaces_node_id"));
    assert!(!index_exists(&conn, "idx_workspaces_status"));
    assert!(!index_exists(&conn, "idx_workspaces_name"));

    // Indexes removed: accounts
    assert!(!index_exists(&conn, "idx_accounts_provider"));
    assert!(!index_exists(&conn, "idx_accounts_is_active"));
    assert!(!index_exists(&conn, "idx_accounts_cooldown"));

    // Indexes removed: agents
    assert!(!index_exists(&conn, "idx_agents_workspace_id"));
    assert!(!index_exists(&conn, "idx_agents_state"));
    assert!(!index_exists(&conn, "idx_agents_account_id"));
    assert!(!index_exists(&conn, "idx_agents_type"));

    // Indexes removed: queue_items
    assert!(!index_exists(&conn, "idx_queue_items_agent_id"));
    assert!(!index_exists(&conn, "idx_queue_items_status"));
    assert!(!index_exists(&conn, "idx_queue_items_position"));

    // Indexes removed: events
    assert!(!index_exists(&conn, "idx_events_timestamp"));
    assert!(!index_exists(&conn, "idx_events_type"));
    assert!(!index_exists(&conn, "idx_events_entity"));
    assert!(!index_exists(&conn, "idx_events_entity_timestamp"));

    // Indexes removed: alerts
    assert!(!index_exists(&conn, "idx_alerts_workspace_id"));
    assert!(!index_exists(&conn, "idx_alerts_agent_id"));
    assert!(!index_exists(&conn, "idx_alerts_is_resolved"));
    assert!(!index_exists(&conn, "idx_alerts_severity"));

    // Indexes removed: transcripts
    assert!(!index_exists(&conn, "idx_transcripts_agent_id"));
    assert!(!index_exists(&conn, "idx_transcripts_captured_at"));
    assert!(!index_exists(&conn, "idx_transcripts_hash"));

    // Indexes removed: approvals
    assert!(!index_exists(&conn, "idx_approvals_agent_id"));
    assert!(!index_exists(&conn, "idx_approvals_status"));

    // Triggers removed
    assert!(!trigger_exists(&conn, "update_nodes_timestamp"));
    assert!(!trigger_exists(&conn, "update_workspaces_timestamp"));
    assert!(!trigger_exists(&conn, "update_agents_timestamp"));
    assert!(!trigger_exists(&conn, "update_accounts_timestamp"));

    drop(conn);

    let _ = std::fs::remove_file(path);
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    object_exists(conn, "table", name)
}

fn index_exists(conn: &Connection, name: &str) -> bool {
    object_exists(conn, "index", name)
}

fn trigger_exists(conn: &Connection, name: &str) -> bool {
    object_exists(conn, "trigger", name)
}

fn object_exists(conn: &Connection, object_type: &str, name: &str) -> bool {
    let row = match conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2 LIMIT 1",
            params![object_type, name],
            |row| row.get::<_, i32>(0),
        )
        .optional()
    {
        Ok(value) => value,
        Err(err) => panic!("sqlite_master query failed: {err}"),
    };
    row.is_some()
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_nanos(),
        Err(err) => panic!("clock before epoch: {err}"),
    };
    let suffix = uuid::Uuid::new_v4();
    std::env::temp_dir().join(format!("forge-db-{prefix}-{nanos}-{suffix}.sqlite"))
}
