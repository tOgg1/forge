use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db, MIGRATIONS};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_011_embedded_sql_matches_go_files() {
    let migration = match MIGRATIONS.iter().find(|entry| entry.version == 11) {
        Some(migration) => migration,
        None => panic!("migration 011 not embedded"),
    };

    let up = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../internal/db/migrations/011_loop_kv.up.sql"
    ));
    let down = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../internal/db/migrations/011_loop_kv.down.sql"
    ));

    assert_eq!(migration.up_sql, up);
    assert_eq!(migration.down_sql, down);
}

#[test]
fn migration_011_up_down_parity() {
    let path = temp_db_path("migration-011");

    // Migrate to 009 first (011 depends on loops table from 007; 010 is skipped).
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(9) {
        panic!("migrate_to(9) failed: {err}");
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

    // Apply migration 011.
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(11) {
        panic!("migrate_to(11) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    // Table exists.
    assert!(table_exists(&conn, "loop_kv"));

    // Columns exist with expected names.
    assert!(column_exists(&conn, "loop_kv", "id"));
    assert!(column_exists(&conn, "loop_kv", "loop_id"));
    assert!(column_exists(&conn, "loop_kv", "key"));
    assert!(column_exists(&conn, "loop_kv", "value"));
    assert!(column_exists(&conn, "loop_kv", "created_at"));
    assert!(column_exists(&conn, "loop_kv", "updated_at"));

    // Index exists.
    assert!(index_exists(&conn, "idx_loop_kv_loop_id"));

    // Trigger exists.
    assert!(trigger_exists(&conn, "update_loop_kv_timestamp"));

    // Insert a kv row and verify defaults.
    if let Err(err) = conn.execute(
        "INSERT INTO loop_kv (id, loop_id, key, value) VALUES (?1, ?2, ?3, ?4)",
        params![
            "kv-001",
            "loop-001",
            "prompt.context",
            "some injected context"
        ],
    ) {
        panic!("insert loop_kv failed: {err}");
    }

    let (created_at, updated_at): (String, String) = match conn.query_row(
        "SELECT created_at, updated_at FROM loop_kv WHERE id = ?1",
        params!["kv-001"],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ) {
        Ok(value) => value,
        Err(err) => panic!("query defaults failed: {err}"),
    };
    assert!(
        !created_at.is_empty(),
        "created_at should be set by default"
    );
    assert!(
        !updated_at.is_empty(),
        "updated_at should be set by default"
    );

    // Verify UNIQUE(loop_id, key) constraint.
    let dup = conn.execute(
        "INSERT INTO loop_kv (id, loop_id, key, value) VALUES (?1, ?2, ?3, ?4)",
        params!["kv-002", "loop-001", "prompt.context", "duplicate key"],
    );
    assert!(dup.is_err(), "duplicate (loop_id, key) should be rejected");

    // Same key in a different loop should succeed.
    if let Err(err) = conn.execute(
        "INSERT INTO loops (id, name, repo_path) VALUES (?1, ?2, ?3)",
        params!["loop-002", "other-loop", "/repo/other"],
    ) {
        panic!("insert second loop failed: {err}");
    }
    if let Err(err) = conn.execute(
        "INSERT INTO loop_kv (id, loop_id, key, value) VALUES (?1, ?2, ?3, ?4)",
        params![
            "kv-003",
            "loop-002",
            "prompt.context",
            "different loop same key"
        ],
    ) {
        panic!("same key in different loop should succeed: {err}");
    }

    // Verify update trigger fires.
    let _before: String = match conn.query_row(
        "SELECT updated_at FROM loop_kv WHERE id = ?1",
        params!["kv-001"],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(err) => panic!("query updated_at before failed: {err}"),
    };

    if let Err(err) = conn.execute(
        "UPDATE loop_kv SET value = ?1 WHERE id = ?2",
        params!["updated context", "kv-001"],
    ) {
        panic!("update loop_kv failed: {err}");
    }

    let after: String = match conn.query_row(
        "SELECT updated_at FROM loop_kv WHERE id = ?1",
        params!["kv-001"],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(err) => panic!("query updated_at after failed: {err}"),
    };
    assert!(
        !after.is_empty(),
        "updated_at should not be empty after trigger"
    );

    // Verify foreign key cascade: deleting the loop should delete kv rows.
    let _ = conn.pragma_update(None, "foreign_keys", "ON");
    if let Err(err) = conn.execute("DELETE FROM loops WHERE id = ?1", params!["loop-001"]) {
        panic!("delete loop failed: {err}");
    }
    let count: i64 = match conn.query_row(
        "SELECT COUNT(1) FROM loop_kv WHERE loop_id = ?1",
        params!["loop-001"],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(err) => panic!("count after cascade failed: {err}"),
    };
    assert_eq!(count, 0, "cascade delete should remove kv rows");

    drop(conn);

    // Rollback migration 011.
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(9) {
        panic!("migrate_to(9) rollback failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    // Table, index, trigger should be gone.
    assert!(!table_exists(&conn, "loop_kv"));
    assert!(!index_exists(&conn, "idx_loop_kv_loop_id"));
    assert!(!trigger_exists(&conn, "update_loop_kv_timestamp"));

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
    std::env::temp_dir().join(format!("forge-db-{prefix}-{nanos}.sqlite"))
}
