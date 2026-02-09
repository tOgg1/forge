use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db};
use rusqlite::{Connection, OptionalExtension};

#[test]
fn migration_003_up_down_parity() {
    let path = temp_db_path("migration-003");

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(3) {
        panic!("migrate_to(3) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    // After migration 003 UP: queue_items should have an attempts column.
    assert!(table_exists(&conn, "queue_items"));
    assert!(column_exists(&conn, "queue_items", "attempts"));
    drop(conn);

    // Migrate back down to 002 (rollback 003).
    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(2) {
        panic!("migrate_to(2) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    // After migration 003 DOWN: queue_items exists (rebuilt) but without attempts.
    assert!(table_exists(&conn, "queue_items"));
    assert!(!column_exists(&conn, "queue_items", "attempts"));

    // The down migration rebuilds the table and re-creates indexes.
    assert!(index_exists(&conn, "idx_queue_items_agent_id"));
    assert!(index_exists(&conn, "idx_queue_items_status"));
    assert!(index_exists(&conn, "idx_queue_items_position"));
    drop(conn);

    let _ = std::fs::remove_file(path);
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    object_exists(conn, "table", name)
}

fn index_exists(conn: &Connection, name: &str) -> bool {
    object_exists(conn, "index", name)
}

fn object_exists(conn: &Connection, object_type: &str, name: &str) -> bool {
    let row = match conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2 LIMIT 1",
            rusqlite::params![object_type, name],
            |row| row.get::<_, i32>(0),
        )
        .optional()
    {
        Ok(value) => value,
        Err(err) => panic!("sqlite_master query failed: {err}"),
    };
    row.is_some()
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = match conn.prepare(&sql) {
        Ok(value) => value,
        Err(err) => panic!("prepare pragma failed: {err}"),
    };

    let mut rows = match stmt.query([]) {
        Ok(value) => value,
        Err(err) => panic!("query pragma failed: {err}"),
    };

    loop {
        let row = match rows.next() {
            Ok(value) => value,
            Err(err) => panic!("iterate pragma failed: {err}"),
        };
        let Some(row) = row else {
            return false;
        };
        let name: String = match row.get(1) {
            Ok(value) => value,
            Err(err) => panic!("read pragma column failed: {err}"),
        };
        if name == column {
            return true;
        }
    }
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_nanos(),
        Err(err) => panic!("clock before epoch: {err}"),
    };
    std::env::temp_dir().join(format!("forge-db-{prefix}-{nanos}.sqlite"))
}
