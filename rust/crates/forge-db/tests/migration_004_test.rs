use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_004_up_down_parity() {
    let path = temp_db_path("migration-004");

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(4) {
        panic!("migrate_to(4) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    assert!(table_exists(&conn, "usage_records"));
    assert!(table_exists(&conn, "daily_usage_cache"));

    assert!(index_exists(&conn, "idx_usage_records_account_id"));
    assert!(index_exists(&conn, "idx_usage_records_agent_id"));
    assert!(index_exists(&conn, "idx_usage_records_session_id"));
    assert!(index_exists(&conn, "idx_usage_records_provider"));
    assert!(index_exists(&conn, "idx_usage_records_recorded_at"));
    assert!(index_exists(&conn, "idx_usage_records_account_day"));
    assert!(index_exists(&conn, "idx_usage_records_provider_time"));
    assert!(index_exists(&conn, "idx_daily_usage_cache_date"));
    assert!(index_exists(&conn, "idx_daily_usage_cache_provider"));
    drop(conn);

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(3) {
        panic!("migrate_to(3) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    assert!(!table_exists(&conn, "usage_records"));
    assert!(!table_exists(&conn, "daily_usage_cache"));

    assert!(!index_exists(&conn, "idx_usage_records_account_id"));
    assert!(!index_exists(&conn, "idx_usage_records_agent_id"));
    assert!(!index_exists(&conn, "idx_usage_records_session_id"));
    assert!(!index_exists(&conn, "idx_usage_records_provider"));
    assert!(!index_exists(&conn, "idx_usage_records_recorded_at"));
    assert!(!index_exists(&conn, "idx_usage_records_account_day"));
    assert!(!index_exists(&conn, "idx_usage_records_provider_time"));
    assert!(!index_exists(&conn, "idx_daily_usage_cache_date"));
    assert!(!index_exists(&conn, "idx_daily_usage_cache_provider"));
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
    std::env::temp_dir().join(format!("forge-db-{prefix}-{nanos}.sqlite"))
}
