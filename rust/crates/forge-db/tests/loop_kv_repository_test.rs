use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::{Config, Db, DbError, LoopKVRepository};
use rusqlite::params;

/// Create a migrated DB with a test loop, return (Db, loop_id).
fn setup_db() -> (Db, String) {
    let path = temp_db_path("loop-kv-repo");
    let mut db = match Db::open(Config::new(&path)) {
        Ok(db) => db,
        Err(err) => panic!("open db: {err}"),
    };
    if let Err(err) = db.migrate_up() {
        panic!("migrate_up: {err}");
    }

    // Insert a loop row that loop_kv entries can reference.
    let loop_id = "loop-test-001";
    if let Err(err) = db.conn().execute(
        "INSERT INTO loops (id, name, repo_path) VALUES (?1, ?2, ?3)",
        params![loop_id, "test-loop", "/repo/test"],
    ) {
        panic!("insert test loop: {err}");
    }
    (db, loop_id.to_string())
}

// -----------------------------------------------------------------------
// Go parity: TestLoopKVRepository_SetGetListDelete
// -----------------------------------------------------------------------

#[test]
fn set_get_list_delete_parity() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);

    // Set creates a new entry.
    if let Err(err) = repo.set(&loop_id, "blocked_on", "waiting for reply") {
        panic!("set (create): {err}");
    }

    // Set with same key updates the value (upsert).
    if let Err(err) = repo.set(&loop_id, "blocked_on", "still waiting") {
        panic!("set (update): {err}");
    }

    // Get returns the updated value.
    let got = match repo.get(&loop_id, "blocked_on") {
        Ok(kv) => kv,
        Err(err) => panic!("get: {err}"),
    };
    assert_eq!(got.key, "blocked_on");
    assert_eq!(got.value, "still waiting");
    assert_eq!(got.loop_id, loop_id);
    assert!(!got.id.is_empty());
    assert!(!got.created_at.is_empty());
    assert!(!got.updated_at.is_empty());

    // ListByLoop returns exactly 1 entry.
    let items = match repo.list_by_loop(&loop_id) {
        Ok(items) => items,
        Err(err) => panic!("list_by_loop: {err}"),
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].key, "blocked_on");
    assert_eq!(items[0].value, "still waiting");

    // Delete removes the entry.
    if let Err(err) = repo.delete(&loop_id, "blocked_on") {
        panic!("delete: {err}");
    }

    // Get after delete returns not found.
    let err = repo.get(&loop_id, "blocked_on");
    assert!(
        matches!(err, Err(DbError::LoopKVNotFound(_))),
        "expected LoopKVNotFound after delete, got: {err:?}"
    );
}

// -----------------------------------------------------------------------
// Validation edge cases
// -----------------------------------------------------------------------

#[test]
fn set_rejects_empty_loop_id() {
    let (db, _) = setup_db();
    let repo = LoopKVRepository::new(&db);
    let result = repo.set("", "key", "value");
    assert!(
        matches!(result, Err(DbError::Validation(_))),
        "expected Validation error for empty loop_id, got: {result:?}"
    );
}

#[test]
fn set_rejects_whitespace_only_loop_id() {
    let (db, _) = setup_db();
    let repo = LoopKVRepository::new(&db);
    let result = repo.set("   ", "key", "value");
    assert!(
        matches!(result, Err(DbError::Validation(_))),
        "expected Validation error for whitespace loop_id, got: {result:?}"
    );
}

#[test]
fn set_rejects_empty_key() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);
    let result = repo.set(&loop_id, "", "value");
    assert!(
        matches!(result, Err(DbError::Validation(_))),
        "expected Validation error for empty key, got: {result:?}"
    );
}

#[test]
fn set_rejects_empty_value() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);
    let result = repo.set(&loop_id, "key", "");
    assert!(
        matches!(result, Err(DbError::Validation(_))),
        "expected Validation error for empty value, got: {result:?}"
    );
}

// -----------------------------------------------------------------------
// Whitespace trimming parity (Go trims loop_id and key)
// -----------------------------------------------------------------------

#[test]
fn set_and_get_trim_loop_id_and_key() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);

    // Set with whitespace-padded loop_id and key.
    let padded_id = format!("  {loop_id}  ");
    let padded_key = "  mykey  ";
    if let Err(err) = repo.set(&padded_id, padded_key, "trimmed") {
        panic!("set with padding: {err}");
    }

    // Get with clean values should find the same entry.
    let got = match repo.get(&loop_id, "mykey") {
        Ok(kv) => kv,
        Err(err) => panic!("get trimmed: {err}"),
    };
    assert_eq!(got.value, "trimmed");

    // Delete with padding should also work.
    if let Err(err) = repo.delete(&padded_id, padded_key) {
        panic!("delete with padding: {err}");
    }
}

// -----------------------------------------------------------------------
// ListByLoop ordering and multi-key
// -----------------------------------------------------------------------

#[test]
fn list_by_loop_returns_sorted_by_key() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);

    if let Err(err) = repo.set(&loop_id, "zebra", "z") {
        panic!("set zebra: {err}");
    }
    if let Err(err) = repo.set(&loop_id, "alpha", "a") {
        panic!("set alpha: {err}");
    }
    if let Err(err) = repo.set(&loop_id, "middle", "m") {
        panic!("set middle: {err}");
    }

    let items = match repo.list_by_loop(&loop_id) {
        Ok(items) => items,
        Err(err) => panic!("list_by_loop: {err}"),
    };
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].key, "alpha");
    assert_eq!(items[1].key, "middle");
    assert_eq!(items[2].key, "zebra");
}

#[test]
fn list_by_loop_empty_returns_empty_vec() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);

    let items = match repo.list_by_loop(&loop_id) {
        Ok(items) => items,
        Err(err) => panic!("list_by_loop: {err}"),
    };
    assert!(items.is_empty());
}

#[test]
fn list_by_loop_isolates_loops() {
    let (db, loop_id) = setup_db();

    // Create a second loop.
    let loop_id2 = "loop-test-002";
    if let Err(err) = db.conn().execute(
        "INSERT INTO loops (id, name, repo_path) VALUES (?1, ?2, ?3)",
        params![loop_id2, "other-loop", "/repo/other"],
    ) {
        panic!("insert second loop: {err}");
    }

    let repo = LoopKVRepository::new(&db);

    if let Err(err) = repo.set(&loop_id, "shared_key", "loop1-val") {
        panic!("set loop1: {err}");
    }
    if let Err(err) = repo.set(loop_id2, "shared_key", "loop2-val") {
        panic!("set loop2: {err}");
    }

    // Each loop sees only its own entry.
    let items1 = match repo.list_by_loop(&loop_id) {
        Ok(items) => items,
        Err(err) => panic!("list loop1: {err}"),
    };
    assert_eq!(items1.len(), 1);
    assert_eq!(items1[0].value, "loop1-val");

    let items2 = match repo.list_by_loop(loop_id2) {
        Ok(items) => items,
        Err(err) => panic!("list loop2: {err}"),
    };
    assert_eq!(items2.len(), 1);
    assert_eq!(items2[0].value, "loop2-val");
}

// -----------------------------------------------------------------------
// Delete edge cases
// -----------------------------------------------------------------------

#[test]
fn delete_nonexistent_key_returns_not_found() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);

    let result = repo.delete(&loop_id, "nonexistent");
    assert!(
        matches!(result, Err(DbError::LoopKVNotFound(_))),
        "expected LoopKVNotFound for nonexistent key, got: {result:?}"
    );
}

#[test]
fn delete_wrong_loop_returns_not_found() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);

    if let Err(err) = repo.set(&loop_id, "key1", "value1") {
        panic!("set: {err}");
    }

    // Delete from a non-existent loop should fail.
    let result = repo.delete("nonexistent-loop", "key1");
    assert!(
        matches!(result, Err(DbError::LoopKVNotFound(_))),
        "expected LoopKVNotFound for wrong loop, got: {result:?}"
    );

    // Original key should still exist.
    let got = match repo.get(&loop_id, "key1") {
        Ok(kv) => kv,
        Err(err) => panic!("get after wrong-loop delete: {err}"),
    };
    assert_eq!(got.value, "value1");
}

// -----------------------------------------------------------------------
// Get edge cases
// -----------------------------------------------------------------------

#[test]
fn get_nonexistent_returns_not_found() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);

    let result = repo.get(&loop_id, "nonexistent");
    assert!(
        matches!(result, Err(DbError::LoopKVNotFound(_))),
        "expected LoopKVNotFound, got: {result:?}"
    );
}

// -----------------------------------------------------------------------
// Set update preserves ID and created_at
// -----------------------------------------------------------------------

#[test]
fn set_update_preserves_id_and_created_at() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);

    if let Err(err) = repo.set(&loop_id, "persist", "v1") {
        panic!("set v1: {err}");
    }
    let first = match repo.get(&loop_id, "persist") {
        Ok(kv) => kv,
        Err(err) => panic!("get v1: {err}"),
    };

    // Update the value.
    if let Err(err) = repo.set(&loop_id, "persist", "v2") {
        panic!("set v2: {err}");
    }
    let second = match repo.get(&loop_id, "persist") {
        Ok(kv) => kv,
        Err(err) => panic!("get v2: {err}"),
    };

    // ID should be the same (UPDATE doesn't change it).
    assert_eq!(first.id, second.id);
    // created_at should be the same.
    assert_eq!(first.created_at, second.created_at);
    // Value should be updated.
    assert_eq!(second.value, "v2");
}

// -----------------------------------------------------------------------
// Timestamp format matches Go RFC3339 (YYYY-MM-DDTHH:MM:SSZ)
// -----------------------------------------------------------------------

#[test]
fn timestamps_are_rfc3339_format() {
    let (db, loop_id) = setup_db();
    let repo = LoopKVRepository::new(&db);

    if let Err(err) = repo.set(&loop_id, "ts_test", "check") {
        panic!("set: {err}");
    }
    let got = match repo.get(&loop_id, "ts_test") {
        Ok(kv) => kv,
        Err(err) => panic!("get: {err}"),
    };

    // Should match YYYY-MM-DDTHH:MM:SSZ pattern.
    assert!(
        got.created_at.ends_with('Z'),
        "created_at should end with Z: {}",
        got.created_at
    );
    assert_eq!(
        got.created_at.len(),
        20,
        "created_at should be 20 chars (RFC3339): {}",
        got.created_at
    );
    assert!(
        got.updated_at.ends_with('Z'),
        "updated_at should end with Z: {}",
        got.updated_at
    );
}

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

fn temp_db_path(prefix: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    std::env::temp_dir().join(format!(
        "forge-db-{prefix}-{nanos}-{}.sqlite",
        std::process::id()
    ))
}
