#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_db::{Config, Db, DbError};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn temp_db_path(tag: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    std::env::temp_dir().join(format!(
        "forge-db-tx-{tag}-{nanos}-{}.sqlite",
        std::process::id()
    ))
}

fn open_migrated(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let mut db = match Db::open(Config::new(&path)) {
        Ok(db) => db,
        Err(e) => panic!("open db: {e}"),
    };
    if let Err(e) = db.migrate_up() {
        panic!("migrate: {e}");
    }
    (db, path)
}

#[test]
fn transaction_commits_on_success() {
    let (mut db, path) = open_migrated("commit");

    db.transaction(|tx| {
        tx.execute("CREATE TABLE tx_commit_test (id TEXT PRIMARY KEY)", [])?;
        tx.execute("INSERT INTO tx_commit_test (id) VALUES (?1)", ["a"])?;
        Ok::<_, DbError>(())
    })
    .unwrap();

    let count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM tx_commit_test", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);

    let _ = std::fs::remove_file(path);
}

#[test]
fn transaction_rolls_back_on_error() {
    let (mut db, path) = open_migrated("rollback");

    db.conn()
        .execute("CREATE TABLE tx_rollback_test (id TEXT PRIMARY KEY)", [])
        .unwrap();

    let err = db
        .transaction(|tx| {
            tx.execute("INSERT INTO tx_rollback_test (id) VALUES (?1)", ["a"])?;
            Err::<(), DbError>(DbError::Validation("boom".into()))
        })
        .unwrap_err();
    assert!(matches!(err, DbError::Validation(_)));

    let count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM tx_rollback_test", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);

    let _ = std::fs::remove_file(path);
}

#[test]
fn transaction_with_retry_retries_on_busy_error_message() {
    let (mut db, path) = open_migrated("retry");

    db.conn()
        .execute("CREATE TABLE tx_retry_test (id TEXT PRIMARY KEY)", [])
        .unwrap();

    let mut attempts = 0;
    db.transaction_with_retry(3, Duration::from_millis(1), |tx| {
        attempts += 1;
        if attempts < 3 {
            return Err(DbError::Validation("database is locked".into()));
        }
        tx.execute("INSERT INTO tx_retry_test (id) VALUES (?1)", ["a"])?;
        Ok::<_, DbError>(())
    })
    .unwrap();

    assert_eq!(attempts, 3);
    let count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM tx_retry_test", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);

    let _ = std::fs::remove_file(path);
}
