use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db, MIGRATIONS};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_008_embedded_sql_matches_go_files() {
    let migration = match MIGRATIONS.iter().find(|entry| entry.version == 8) {
        Some(migration) => migration,
        None => panic!("migration 008 not embedded"),
    };

    let up = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../internal/db/migrations/008_loop_short_id.up.sql"
    ));
    let down = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../internal/db/migrations/008_loop_short_id.down.sql"
    ));

    assert_eq!(migration.up_sql, up);
    assert_eq!(migration.down_sql, down);
}

#[test]
fn migration_008_up_down_parity() {
    let path = temp_db_path("migration-008");

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(7) {
        panic!("migrate_to(7) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    if let Err(err) = conn.execute(
        "INSERT INTO loops (id, name, repo_path) VALUES (?1, ?2, ?3)",
        params![
            "ABCDEF12-0000-0000-0000-000000000000",
            "alpha",
            "/repo/alpha"
        ],
    ) {
        panic!("insert alpha loop failed: {err}");
    }
    if let Err(err) = conn.execute(
        "INSERT INTO loops (id, name, repo_path) VALUES (?1, ?2, ?3)",
        params!["12345678-9999-9999-9999-999999999999", "beta", "/repo/beta"],
    ) {
        panic!("insert beta loop failed: {err}");
    }
    drop(conn);

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(8) {
        panic!("migrate_to(8) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    assert!(column_exists(&conn, "loops", "short_id"));
    assert!(index_exists(&conn, "idx_loops_short_id"));

    let alpha_short_id = query_short_id(&conn, "ABCDEF12-0000-0000-0000-000000000000");
    let beta_short_id = query_short_id(&conn, "12345678-9999-9999-9999-999999999999");
    assert_eq!(alpha_short_id, "abcdef12");
    assert_eq!(beta_short_id, "12345678");

    let duplicate = conn.execute(
        "UPDATE loops SET short_id = ?1 WHERE id = ?2",
        params!["abcdef12", "12345678-9999-9999-9999-999999999999"],
    );
    assert!(duplicate.is_err());

    drop(conn);

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(7) {
        panic!("migrate_to(7) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    assert!(!column_exists(&conn, "loops", "short_id"));
    assert!(!index_exists(&conn, "idx_loops_short_id"));

    let count: i64 = match conn.query_row("SELECT COUNT(1) FROM loops", [], |row| row.get(0)) {
        Ok(value) => value,
        Err(err) => panic!("count loops failed: {err}"),
    };
    assert_eq!(count, 2);
    drop(conn);

    let _ = std::fs::remove_file(path);
}

fn query_short_id(conn: &Connection, loop_id: &str) -> String {
    match conn.query_row(
        "SELECT short_id FROM loops WHERE id = ?1",
        params![loop_id],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(err) => panic!("query short_id failed: {err}"),
    }
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

fn index_exists(conn: &Connection, name: &str) -> bool {
    let row = match conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2 LIMIT 1",
            params!["index", name],
            |row| row.get::<_, i32>(0),
        )
        .optional()
    {
        Ok(value) => value,
        Err(err) => panic!("sqlite_master index query failed: {err}"),
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
