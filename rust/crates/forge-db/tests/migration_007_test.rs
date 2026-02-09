use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db, MIGRATIONS};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_007_embedded_sql_matches_go_files() {
    let migration = match MIGRATIONS.iter().find(|entry| entry.version == 7) {
        Some(migration) => migration,
        None => panic!("migration 007 not embedded"),
    };

    let up = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../internal/db/migrations/007_loop_runtime.up.sql"
    ));
    let down = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../internal/db/migrations/007_loop_runtime.down.sql"
    ));

    assert_eq!(migration.up_sql, up);
    assert_eq!(migration.down_sql, down);
}

#[test]
fn migration_007_up_down_parity() {
    let path = temp_db_path("migration-007");

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

    assert!(table_exists(&conn, "profiles"));
    assert!(table_exists(&conn, "pools"));
    assert!(table_exists(&conn, "pool_members"));
    assert!(table_exists(&conn, "loops"));
    assert!(table_exists(&conn, "loop_queue_items"));
    assert!(table_exists(&conn, "loop_runs"));

    assert!(index_exists(&conn, "idx_profiles_harness"));
    assert!(index_exists(&conn, "idx_profiles_cooldown"));
    assert!(index_exists(&conn, "idx_pools_default"));
    assert!(index_exists(&conn, "idx_pool_members_pool_id"));
    assert!(index_exists(&conn, "idx_pool_members_profile_id"));
    assert!(index_exists(&conn, "idx_loops_repo_path"));
    assert!(index_exists(&conn, "idx_loops_state"));
    assert!(index_exists(&conn, "idx_loops_pool_id"));
    assert!(index_exists(&conn, "idx_loops_profile_id"));
    assert!(index_exists(&conn, "idx_loop_queue_items_loop_id"));
    assert!(index_exists(&conn, "idx_loop_queue_items_status"));
    assert!(index_exists(&conn, "idx_loop_queue_items_position"));
    assert!(index_exists(&conn, "idx_loop_runs_loop_id"));
    assert!(index_exists(&conn, "idx_loop_runs_profile_id"));
    assert!(index_exists(&conn, "idx_loop_runs_status"));

    assert!(trigger_exists(&conn, "update_profiles_timestamp"));
    assert!(trigger_exists(&conn, "update_pools_timestamp"));
    assert!(trigger_exists(&conn, "update_loops_timestamp"));

    let loops_sql = table_sql(&conn, "loops");
    assert!(loops_sql.contains("interval_seconds INTEGER NOT NULL DEFAULT 30"));
    assert!(loops_sql.contains("state TEXT NOT NULL DEFAULT 'stopped'"));

    let queue_sql = table_sql(&conn, "loop_queue_items");
    assert!(queue_sql.contains("'dispatched'"));
    assert!(queue_sql.contains("'skipped'"));
    assert!(queue_sql.contains("attempts INTEGER NOT NULL DEFAULT 0"));

    drop(conn);

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(6) {
        panic!("migrate_to(6) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };

    assert!(!table_exists(&conn, "profiles"));
    assert!(!table_exists(&conn, "pools"));
    assert!(!table_exists(&conn, "pool_members"));
    assert!(!table_exists(&conn, "loops"));
    assert!(!table_exists(&conn, "loop_queue_items"));
    assert!(!table_exists(&conn, "loop_runs"));

    assert!(!index_exists(&conn, "idx_profiles_harness"));
    assert!(!index_exists(&conn, "idx_profiles_cooldown"));
    assert!(!index_exists(&conn, "idx_pools_default"));
    assert!(!index_exists(&conn, "idx_pool_members_pool_id"));
    assert!(!index_exists(&conn, "idx_pool_members_profile_id"));
    assert!(!index_exists(&conn, "idx_loops_repo_path"));
    assert!(!index_exists(&conn, "idx_loops_state"));
    assert!(!index_exists(&conn, "idx_loops_pool_id"));
    assert!(!index_exists(&conn, "idx_loops_profile_id"));
    assert!(!index_exists(&conn, "idx_loop_queue_items_loop_id"));
    assert!(!index_exists(&conn, "idx_loop_queue_items_status"));
    assert!(!index_exists(&conn, "idx_loop_queue_items_position"));
    assert!(!index_exists(&conn, "idx_loop_runs_loop_id"));
    assert!(!index_exists(&conn, "idx_loop_runs_profile_id"));
    assert!(!index_exists(&conn, "idx_loop_runs_status"));

    assert!(!trigger_exists(&conn, "update_profiles_timestamp"));
    assert!(!trigger_exists(&conn, "update_pools_timestamp"));
    assert!(!trigger_exists(&conn, "update_loops_timestamp"));

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
        Err(err) => panic!("sqlite_master query failed: {err}"),
    };
    row.is_some()
}

fn table_sql(conn: &Connection, table: &str) -> String {
    match conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![table],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(err) => panic!("read table sql failed: {err}"),
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
