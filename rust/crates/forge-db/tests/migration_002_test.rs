use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db};
use rusqlite::Connection;

#[test]
fn migration_002_up_down_parity() {
    let path = temp_db_path("migration-002");

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(2) {
        panic!("migrate_to(2) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    assert!(column_exists(&conn, "nodes", "ssh_agent_forwarding"));
    assert!(column_exists(&conn, "nodes", "ssh_proxy_jump"));
    assert!(column_exists(&conn, "nodes", "ssh_control_master"));
    assert!(column_exists(&conn, "nodes", "ssh_control_path"));
    assert!(column_exists(&conn, "nodes", "ssh_control_persist"));
    assert!(column_exists(&conn, "nodes", "ssh_timeout_seconds"));
    drop(conn);

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(1) {
        panic!("migrate_to(1) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    assert!(!column_exists(&conn, "nodes", "ssh_agent_forwarding"));
    assert!(!column_exists(&conn, "nodes", "ssh_proxy_jump"));
    assert!(!column_exists(&conn, "nodes", "ssh_control_master"));
    assert!(!column_exists(&conn, "nodes", "ssh_control_path"));
    assert!(!column_exists(&conn, "nodes", "ssh_control_persist"));
    assert!(!column_exists(&conn, "nodes", "ssh_timeout_seconds"));
    drop(conn);

    let _ = std::fs::remove_file(path);
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
    let suffix = uuid::Uuid::new_v4();
    std::env::temp_dir().join(format!("forge-db-{prefix}-{nanos}-{suffix}.sqlite"))
}
