use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_006_up_down_parity() {
    let path = temp_db_path("migration-006");

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(6) {
        panic!("migrate_to(6) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    assert!(table_exists(&conn, "mail_threads"));
    assert!(table_exists(&conn, "mail_messages"));
    assert!(table_exists(&conn, "file_locks"));

    assert!(index_exists(&conn, "idx_mail_threads_workspace_id"));
    assert!(index_exists(&conn, "idx_mail_messages_thread_id"));
    assert!(index_exists(&conn, "idx_mail_messages_recipient"));
    assert!(index_exists(&conn, "idx_mail_messages_unread"));
    assert!(index_exists(&conn, "idx_file_locks_active"));
    assert!(index_exists(&conn, "idx_file_locks_path"));
    drop(conn);

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(5) {
        panic!("migrate_to(5) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    assert!(!table_exists(&conn, "mail_threads"));
    assert!(!table_exists(&conn, "mail_messages"));
    assert!(!table_exists(&conn, "file_locks"));

    assert!(!index_exists(&conn, "idx_mail_threads_workspace_id"));
    assert!(!index_exists(&conn, "idx_mail_messages_thread_id"));
    assert!(!index_exists(&conn, "idx_mail_messages_recipient"));
    assert!(!index_exists(&conn, "idx_mail_messages_unread"));
    assert!(!index_exists(&conn, "idx_file_locks_active"));
    assert!(!index_exists(&conn, "idx_file_locks_path"));
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
