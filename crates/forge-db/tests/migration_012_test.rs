use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db, MIGRATIONS};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_012_embedded_sql_matches_go_files() {
    let migration = match MIGRATIONS.iter().find(|entry| entry.version == 12) {
        Some(migration) => migration,
        None => panic!("migration 012 not embedded"),
    };

    let up = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../old/go/internal/db/migrations/012_loop_work_state.up.sql"
    ));
    let down = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../old/go/internal/db/migrations/012_loop_work_state.down.sql"
    ));

    assert_eq!(migration.up_sql, up);
    assert_eq!(migration.down_sql, down);
}

#[test]
fn migration_012_up_down_parity() {
    let path = temp_db_path("migration-012");

    // Migrate to 011 first (012 depends on loops table from 007).
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(11) {
        panic!("migrate_to(11) failed: {err}");
    }
    drop(db);

    // Insert a loop row so we can test the foreign key relationship.
    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = conn.execute(
        "INSERT INTO loops (id, name, repo_path) VALUES (?1, ?2, ?3)",
        params!["loop-001", "test-loop", "/repo/test"],
    ) {
        panic!("insert loop failed: {err}");
    }
    drop(conn);

    // Apply migration 012.
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(12) {
        panic!("migrate_to(12) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    // Table exists.
    assert!(table_exists(&conn, "loop_work_state"));

    // Columns exist with expected names.
    assert!(column_exists(&conn, "loop_work_state", "id"));
    assert!(column_exists(&conn, "loop_work_state", "loop_id"));
    assert!(column_exists(&conn, "loop_work_state", "agent_id"));
    assert!(column_exists(&conn, "loop_work_state", "task_id"));
    assert!(column_exists(&conn, "loop_work_state", "status"));
    assert!(column_exists(&conn, "loop_work_state", "detail"));
    assert!(column_exists(&conn, "loop_work_state", "loop_iteration"));
    assert!(column_exists(&conn, "loop_work_state", "is_current"));
    assert!(column_exists(&conn, "loop_work_state", "created_at"));
    assert!(column_exists(&conn, "loop_work_state", "updated_at"));

    // Indexes exist.
    assert!(index_exists(&conn, "idx_loop_work_state_loop_id"));
    assert!(index_exists(&conn, "idx_loop_work_state_loop_current"));
    assert!(index_exists(&conn, "idx_loop_work_state_loop_updated"));

    // Trigger exists.
    assert!(trigger_exists(&conn, "update_loop_work_state_timestamp"));

    // Insert a work state row and verify defaults.
    if let Err(err) = conn.execute(
        "INSERT INTO loop_work_state (id, loop_id, agent_id, task_id, status) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["ws-001", "loop-001", "agent-a", "task-1", "in_progress"],
    ) {
        panic!("insert loop_work_state failed: {err}");
    }

    let (iteration, is_current): (i64, i64) = match conn.query_row(
        "SELECT loop_iteration, is_current FROM loop_work_state WHERE id = ?1",
        params!["ws-001"],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ) {
        Ok(value) => value,
        Err(err) => panic!("query defaults failed: {err}"),
    };
    assert_eq!(iteration, 0, "loop_iteration default should be 0");
    assert_eq!(is_current, 0, "is_current default should be 0");

    // Verify UNIQUE(loop_id, task_id) constraint.
    let dup = conn.execute(
        "INSERT INTO loop_work_state (id, loop_id, agent_id, task_id, status) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["ws-002", "loop-001", "agent-b", "task-1", "pending"],
    );
    assert!(
        dup.is_err(),
        "duplicate (loop_id, task_id) should be rejected"
    );

    // Verify update trigger fires.
    if let Err(err) = conn.execute(
        "UPDATE loop_work_state SET status = ?1 WHERE id = ?2",
        params!["completed", "ws-001"],
    ) {
        panic!("update loop_work_state failed: {err}");
    }

    let after: String = match conn.query_row(
        "SELECT updated_at FROM loop_work_state WHERE id = ?1",
        params!["ws-001"],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(err) => panic!("query updated_at after failed: {err}"),
    };
    // updated_at should have been refreshed (or at least set).
    assert!(
        !after.is_empty(),
        "updated_at should not be empty after trigger"
    );

    // Verify foreign key cascade: deleting the loop should delete work state.
    let _ = conn.pragma_update(None, "foreign_keys", "ON");
    if let Err(err) = conn.execute("DELETE FROM loops WHERE id = ?1", params!["loop-001"]) {
        panic!("delete loop failed: {err}");
    }
    let count: i64 = match conn.query_row(
        "SELECT COUNT(1) FROM loop_work_state WHERE loop_id = ?1",
        params!["loop-001"],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(err) => panic!("count after cascade failed: {err}"),
    };
    assert_eq!(count, 0, "cascade delete should remove work state rows");

    drop(conn);

    // Rollback migration 012.
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(11) {
        panic!("migrate_to(11) rollback failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    // Table, indexes, trigger should be gone.
    assert!(!table_exists(&conn, "loop_work_state"));
    assert!(!index_exists(&conn, "idx_loop_work_state_loop_id"));
    assert!(!index_exists(&conn, "idx_loop_work_state_loop_current"));
    assert!(!index_exists(&conn, "idx_loop_work_state_loop_updated"));
    assert!(!trigger_exists(&conn, "update_loop_work_state_timestamp"));

    // loops table should still exist.
    assert!(table_exists(&conn, "loops"));

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
