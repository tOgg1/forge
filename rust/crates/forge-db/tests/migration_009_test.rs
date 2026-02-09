use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db, MIGRATIONS};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_009_embedded_sql_matches_go_files() {
    let migration = match MIGRATIONS.iter().find(|entry| entry.version == 9) {
        Some(migration) => migration,
        None => panic!("migration 009 not embedded"),
    };

    let up = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../internal/db/migrations/009_loop_limits.up.sql"
    ));
    let down = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../internal/db/migrations/009_loop_limits.down.sql"
    ));

    assert_eq!(migration.up_sql, up);
    assert_eq!(migration.down_sql, down);
}

#[test]
fn migration_009_up_down_parity() {
    let path = temp_db_path("migration-009");

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(9) {
        panic!("migrate_to(9) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    assert!(column_exists(&conn, "loops", "max_iterations"));
    assert!(column_exists(&conn, "loops", "max_runtime_seconds"));
    assert_eq!(
        column_default(&conn, "loops", "max_iterations"),
        Some("0".to_string())
    );
    assert_eq!(
        column_default(&conn, "loops", "max_runtime_seconds"),
        Some("0".to_string())
    );

    if let Err(err) = conn.execute(
        "INSERT INTO loops (id, short_id, name, repo_path, max_iterations, max_runtime_seconds)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            "99999999-0000-0000-0000-000000000000",
            "99999999",
            "limits-loop",
            "/repo/limits",
            17,
            900
        ],
    ) {
        panic!("insert loop with limits failed: {err}");
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

    assert!(!column_exists(&conn, "loops", "max_iterations"));
    assert!(!column_exists(&conn, "loops", "max_runtime_seconds"));

    let short_id: String = match conn.query_row(
        "SELECT short_id FROM loops WHERE id = ?1",
        params!["99999999-0000-0000-0000-000000000000"],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(err) => panic!("query loop short_id failed: {err}"),
    };
    assert_eq!(short_id, "99999999");

    assert!(index_exists(&conn, "idx_loops_short_id"));
    assert!(trigger_exists(&conn, "update_loops_timestamp"));
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

fn column_default(conn: &Connection, table: &str, column: &str) -> Option<String> {
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
        let row = row?;

        let name: String = match row.get(1) {
            Ok(value) => value,
            Err(err) => panic!("read pragma column name failed: {err}"),
        };
        if name == column {
            let default = match row.get::<_, Option<String>>(4) {
                Ok(value) => value,
                Err(err) => panic!("read pragma default failed: {err}"),
            };
            return default;
        }
    }
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
