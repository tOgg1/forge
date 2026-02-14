#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db, MIGRATIONS};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_015_embedded_sql_matches_go_files() {
    let migration = match MIGRATIONS.iter().find(|entry| entry.version == 15) {
        Some(migration) => migration,
        None => panic!("migration 015 not embedded"),
    };

    let up = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../old/go/internal/db/migrations/015_team_tasks.up.sql"
    ));
    let down = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../old/go/internal/db/migrations/015_team_tasks.down.sql"
    ));

    assert_eq!(migration.up_sql, up);
    assert_eq!(migration.down_sql, down);
}

#[test]
fn migration_015_up_down_parity() {
    let path = temp_db_path("migration-015");

    let mut db = Db::open(Config::new(&path)).unwrap_or_else(|err| panic!("open db: {err}"));
    db.migrate_to(14)
        .unwrap_or_else(|err| panic!("migrate_to(14): {err}"));
    drop(db);

    let mut db = Db::open(Config::new(&path)).unwrap_or_else(|err| panic!("open db: {err}"));
    db.migrate_to(15)
        .unwrap_or_else(|err| panic!("migrate_to(15): {err}"));
    drop(db);

    let conn = Connection::open(&path).unwrap_or_else(|err| panic!("open sqlite: {err}"));
    assert!(table_exists(&conn, "team_tasks"));
    assert!(table_exists(&conn, "team_task_events"));
    assert!(index_exists(&conn, "idx_team_tasks_team_status_priority"));
    assert!(index_exists(&conn, "idx_team_tasks_assigned_agent"));
    assert!(index_exists(&conn, "idx_team_tasks_updated"));
    assert!(index_exists(&conn, "idx_team_task_events_task_created"));
    assert!(index_exists(&conn, "idx_team_task_events_team_created"));
    assert!(trigger_exists(&conn, "update_team_tasks_timestamp"));

    if let Err(err) = conn.execute(
        "INSERT INTO teams (id, name, heartbeat_interval_seconds) VALUES (?1, ?2, ?3)",
        params!["team-a", "ops-a", 30],
    ) {
        panic!("insert team failed: {err}");
    }

    if let Err(err) = conn.execute(
        "INSERT INTO team_tasks (id, team_id, payload_json, status, priority)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "task-1",
            "team-a",
            r#"{"type":"triage","title":"incident"}"#,
            "queued",
            10
        ],
    ) {
        panic!("insert team task failed: {err}");
    }

    let bad_status = conn.execute(
        "INSERT INTO team_tasks (id, team_id, payload_json, status, priority)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "task-2",
            "team-a",
            r#"{"type":"triage","title":"incident"}"#,
            "bad",
            10
        ],
    );
    assert!(bad_status.is_err(), "invalid status should fail check");

    if let Err(err) = conn.execute(
        "INSERT INTO team_task_events (task_id, team_id, event_type, to_status)
         VALUES (?1, ?2, ?3, ?4)",
        params!["task-1", "team-a", "submitted", "queued"],
    ) {
        panic!("insert team task event failed: {err}");
    }

    let bad_event = conn.execute(
        "INSERT INTO team_task_events (task_id, team_id, event_type, to_status)
         VALUES (?1, ?2, ?3, ?4)",
        params!["task-1", "team-a", "unknown", "queued"],
    );
    assert!(bad_event.is_err(), "invalid event_type should fail check");

    drop(conn);

    let mut db = Db::open(Config::new(&path)).unwrap_or_else(|err| panic!("open db: {err}"));
    db.migrate_to(14)
        .unwrap_or_else(|err| panic!("migrate_to(14): {err}"));
    drop(db);

    let conn = Connection::open(&path).unwrap_or_else(|err| panic!("open sqlite: {err}"));
    assert!(!table_exists(&conn, "team_tasks"));
    assert!(!table_exists(&conn, "team_task_events"));
    assert!(!index_exists(&conn, "idx_team_tasks_team_status_priority"));
    assert!(!index_exists(&conn, "idx_team_tasks_assigned_agent"));
    assert!(!index_exists(&conn, "idx_team_tasks_updated"));
    assert!(!index_exists(&conn, "idx_team_task_events_task_created"));
    assert!(!index_exists(&conn, "idx_team_task_events_team_created"));
    assert!(!trigger_exists(&conn, "update_team_tasks_timestamp"));
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
    let row = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2 LIMIT 1",
            params![object_type, name],
            |row| row.get::<_, i32>(0),
        )
        .optional()
        .unwrap_or_else(|err| panic!("sqlite_master query failed: {err}"));
    row.is_some()
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|err| panic!("clock before epoch: {err}"))
        .as_nanos();
    let suffix = uuid::Uuid::new_v4();
    std::env::temp_dir().join(format!("forge-db-{prefix}-{nanos}-{suffix}.sqlite"))
}
