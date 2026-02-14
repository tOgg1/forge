#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db, MIGRATIONS};
use rusqlite::{params, Connection, OptionalExtension};

#[test]
fn migration_014_embedded_sql_matches_go_files() {
    let migration = match MIGRATIONS.iter().find(|entry| entry.version == 14) {
        Some(migration) => migration,
        None => panic!("migration 014 not embedded"),
    };

    let up = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../old/go/internal/db/migrations/014_team_model.up.sql"
    ));
    let down = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../old/go/internal/db/migrations/014_team_model.down.sql"
    ));

    assert_eq!(migration.up_sql, up);
    assert_eq!(migration.down_sql, down);
}

#[test]
fn migration_014_up_down_parity() {
    let path = temp_db_path("migration-014");

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("open db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(13) {
        panic!("migrate_to(13) failed: {err}");
    }
    drop(db);

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db failed: {err}"),
    };
    if let Err(err) = db.migrate_to(14) {
        panic!("migrate_to(14) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("open sqlite connection failed: {err}"),
    };

    assert!(table_exists(&conn, "teams"));
    assert!(table_exists(&conn, "team_members"));
    assert!(index_exists(&conn, "idx_teams_name"));
    assert!(index_exists(&conn, "idx_teams_default_assignee"));
    assert!(index_exists(&conn, "idx_team_members_team"));
    assert!(index_exists(&conn, "idx_team_members_agent"));
    assert!(index_exists(&conn, "idx_team_members_role"));
    assert!(trigger_exists(&conn, "update_teams_timestamp"));

    for col in &[
        "id",
        "name",
        "delegation_rules_json",
        "default_assignee",
        "heartbeat_interval_seconds",
        "created_at",
        "updated_at",
    ] {
        assert!(
            column_exists(&conn, "teams", col),
            "teams column missing: {col}"
        );
    }
    for col in &["id", "team_id", "agent_id", "role", "created_at"] {
        assert!(
            column_exists(&conn, "team_members", col),
            "team_members column missing: {col}"
        );
    }

    if let Err(err) = conn.execute(
        "INSERT INTO teams (id, name, delegation_rules_json, default_assignee, heartbeat_interval_seconds)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["team-1", "ops-core", r#"{"critical":"lead"}"#, "agent-lead", 60],
    ) {
        panic!("insert team failed: {err}");
    }

    let bad_interval = conn.execute(
        "INSERT INTO teams (id, name, heartbeat_interval_seconds) VALUES (?1, ?2, ?3)",
        params!["team-2", "ops-bad", 0],
    );
    assert!(bad_interval.is_err(), "CHECK interval should reject 0");

    if let Err(err) = conn.execute(
        "INSERT INTO team_members (id, team_id, agent_id, role) VALUES (?1, ?2, ?3, ?4)",
        params!["tm-1", "team-1", "agent-lead", "leader"],
    ) {
        panic!("insert leader member failed: {err}");
    }
    let bad_role = conn.execute(
        "INSERT INTO team_members (id, team_id, agent_id, role) VALUES (?1, ?2, ?3, ?4)",
        params!["tm-2", "team-1", "agent-a", "invalid"],
    );
    assert!(bad_role.is_err(), "invalid role should fail CHECK");

    let duplicate_member = conn.execute(
        "INSERT INTO team_members (id, team_id, agent_id, role) VALUES (?1, ?2, ?3, ?4)",
        params!["tm-3", "team-1", "agent-lead", "leader"],
    );
    assert!(
        duplicate_member.is_err(),
        "unique(team_id, agent_id) should reject duplicates"
    );

    drop(conn);

    let mut db = match Db::open(Config::new(&path)) {
        Ok(value) => value,
        Err(err) => panic!("reopen db for rollback failed: {err}"),
    };
    if let Err(err) = db.migrate_to(13) {
        panic!("rollback migrate_to(13) failed: {err}");
    }
    drop(db);

    let conn = match Connection::open(&path) {
        Ok(value) => value,
        Err(err) => panic!("open sqlite connection after rollback failed: {err}"),
    };
    assert!(!table_exists(&conn, "teams"));
    assert!(!table_exists(&conn, "team_members"));
    assert!(!index_exists(&conn, "idx_teams_name"));
    assert!(!index_exists(&conn, "idx_teams_default_assignee"));
    assert!(!index_exists(&conn, "idx_team_members_team"));
    assert!(!index_exists(&conn, "idx_team_members_agent"));
    assert!(!index_exists(&conn, "idx_team_members_role"));
    assert!(!trigger_exists(&conn, "update_teams_timestamp"));

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
