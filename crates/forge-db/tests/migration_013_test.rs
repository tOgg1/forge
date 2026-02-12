#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db, MIGRATIONS};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_013_embedded_sql_matches_go_files() {
    let migration = match MIGRATIONS.iter().find(|entry| entry.version == 13) {
        Some(migration) => migration,
        None => panic!("migration 013 not embedded"),
    };

    let up = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../old/go/internal/db/migrations/013_persistent_agents.up.sql"
    ));
    let down = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../old/go/internal/db/migrations/013_persistent_agents.down.sql"
    ));

    assert_eq!(migration.up_sql, up);
    assert_eq!(migration.down_sql, down);
}

#[test]
fn migration_013_up_down_parity() {
    let path = temp_db_path("migration-013");

    // Migrate to 12 first (013 is additive, no FK deps on prior loop tables).
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(12) {
        panic!("migrate_to(12) failed: {err}");
    }
    drop(db);

    // Apply migration 013.
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(13) {
        panic!("migrate_to(13) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    // Tables exist.
    assert!(table_exists(&conn, "persistent_agents"));
    assert!(table_exists(&conn, "persistent_agent_events"));

    // persistent_agents columns.
    for col in &[
        "id",
        "parent_agent_id",
        "workspace_id",
        "repo",
        "node",
        "harness",
        "mode",
        "state",
        "ttl_seconds",
        "labels_json",
        "tags_json",
        "created_at",
        "last_activity_at",
        "updated_at",
    ] {
        assert!(
            column_exists(&conn, "persistent_agents", col),
            "persistent_agents should have column {col}"
        );
    }

    // persistent_agent_events columns.
    for col in &["id", "agent_id", "kind", "outcome", "detail", "timestamp"] {
        assert!(
            column_exists(&conn, "persistent_agent_events", col),
            "persistent_agent_events should have column {col}"
        );
    }

    // Indexes exist.
    assert!(index_exists(&conn, "idx_persistent_agents_workspace"));
    assert!(index_exists(&conn, "idx_persistent_agents_state"));
    assert!(index_exists(&conn, "idx_persistent_agents_parent"));
    assert!(index_exists(&conn, "idx_persistent_agents_updated"));
    assert!(index_exists(&conn, "idx_persistent_agents_harness"));
    assert!(index_exists(&conn, "idx_persistent_agent_events_agent"));
    assert!(index_exists(&conn, "idx_persistent_agent_events_kind"));
    assert!(index_exists(&conn, "idx_persistent_agent_events_timestamp"));
    assert!(index_exists(&conn, "idx_persistent_agent_events_agent_ts"));

    // Trigger exists.
    assert!(trigger_exists(&conn, "update_persistent_agents_timestamp"));

    // Insert a persistent agent and verify defaults.
    if let Err(err) = conn.execute(
        "INSERT INTO persistent_agents (id, workspace_id, harness, mode, state) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["pa-001", "ws-test", "claude-code", "continuous", "idle"],
    ) {
        panic!("insert persistent_agents failed: {err}");
    }

    let (created, activity, updated): (String, String, String) = match conn.query_row(
        "SELECT created_at, last_activity_at, updated_at FROM persistent_agents WHERE id = ?1",
        params!["pa-001"],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ) {
        Ok(value) => value,
        Err(err) => panic!("query defaults failed: {err}"),
    };
    assert!(!created.is_empty(), "created_at should have a default");
    assert!(
        !activity.is_empty(),
        "last_activity_at should have a default"
    );
    assert!(!updated.is_empty(), "updated_at should have a default");

    // Mode CHECK constraint: invalid mode should fail.
    let bad_mode = conn.execute(
        "INSERT INTO persistent_agents (id, workspace_id, harness, mode, state) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["pa-bad-mode", "ws-test", "codex", "invalid", "idle"],
    );
    assert!(
        bad_mode.is_err(),
        "invalid mode should violate CHECK constraint"
    );

    // State CHECK constraint: invalid state should fail.
    let bad_state = conn.execute(
        "INSERT INTO persistent_agents (id, workspace_id, harness, mode, state) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "pa-bad-state",
            "ws-test",
            "codex",
            "continuous",
            "invalid_state"
        ],
    );
    assert!(
        bad_state.is_err(),
        "invalid state should violate CHECK constraint"
    );

    // Verify update trigger fires.
    if let Err(err) = conn.execute(
        "UPDATE persistent_agents SET state = ?1 WHERE id = ?2",
        params!["stopped", "pa-001"],
    ) {
        panic!("update persistent_agents failed: {err}");
    }
    let after_updated: String = match conn.query_row(
        "SELECT updated_at FROM persistent_agents WHERE id = ?1",
        params!["pa-001"],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(err) => panic!("query updated_at after trigger failed: {err}"),
    };
    assert!(
        !after_updated.is_empty(),
        "updated_at should be set after trigger"
    );

    // Insert an event and verify AUTOINCREMENT.
    if let Err(err) = conn.execute(
        "INSERT INTO persistent_agent_events (agent_id, kind, outcome, detail) \
         VALUES (?1, ?2, ?3, ?4)",
        params!["pa-001", "spawn", "success", "spawned agent"],
    ) {
        panic!("insert persistent_agent_events failed: {err}");
    }

    let event_id: i64 = match conn.query_row(
        "SELECT id FROM persistent_agent_events WHERE agent_id = ?1",
        params!["pa-001"],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(err) => panic!("query event id failed: {err}"),
    };
    assert!(event_id > 0, "event id should be auto-incremented positive");

    drop(conn);

    // Rollback migration 013.
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(12) {
        panic!("migrate_to(12) rollback failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    // Tables should be gone.
    assert!(!table_exists(&conn, "persistent_agents"));
    assert!(!table_exists(&conn, "persistent_agent_events"));

    // Indexes should be gone.
    assert!(!index_exists(&conn, "idx_persistent_agents_workspace"));
    assert!(!index_exists(&conn, "idx_persistent_agents_state"));
    assert!(!index_exists(&conn, "idx_persistent_agents_parent"));
    assert!(!index_exists(&conn, "idx_persistent_agents_updated"));
    assert!(!index_exists(&conn, "idx_persistent_agents_harness"));
    assert!(!index_exists(&conn, "idx_persistent_agent_events_agent"));
    assert!(!index_exists(&conn, "idx_persistent_agent_events_kind"));
    assert!(!index_exists(
        &conn,
        "idx_persistent_agent_events_timestamp"
    ));
    assert!(!index_exists(&conn, "idx_persistent_agent_events_agent_ts"));

    // Trigger should be gone.
    assert!(!trigger_exists(&conn, "update_persistent_agents_timestamp"));

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
        Err(err) => panic!("sqlite_master query ({object_type}/{name}) failed: {err}"),
    };
    row.is_some()
}

fn column_exists(conn: &Connection, table_name: &str, column_name: &str) -> bool {
    let pragma = format!("PRAGMA table_info({table_name})");
    let mut stmt = match conn.prepare(&pragma) {
        Ok(stmt) => stmt,
        Err(err) => panic!("prepare table_info failed: {err}"),
    };

    let rows = match stmt.query_map([], |row| row.get::<_, String>(1)) {
        Ok(rows) => rows,
        Err(err) => panic!("query table_info failed: {err}"),
    };

    for row in rows {
        let name = match row {
            Ok(value) => value,
            Err(err) => panic!("read table_info row failed: {err}"),
        };
        if name == column_name {
            return true;
        }
    }
    false
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_nanos(),
        Err(err) => panic!("clock before epoch: {err}"),
    };
    let suffix = uuid::Uuid::new_v4();
    std::env::temp_dir().join(format!("forge-db-{prefix}-{nanos}-{suffix}.sqlite"))
}
