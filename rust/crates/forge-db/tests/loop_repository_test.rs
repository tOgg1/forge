#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::loop_repository::{Loop, LoopRepository, LoopState};
use forge_db::{Config, Db, DbError};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_db() -> (Db, PathBuf) {
    let path = temp_db_path("loop-repo");
    let mut db = Db::open(Config::new(&path)).expect("open db");
    db.migrate_up().expect("migrate_up");
    (db, path)
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_nanos(),
        Err(err) => panic!("clock before epoch: {err}"),
    };
    std::env::temp_dir().join(format!(
        "forge-db-{prefix}-{nanos}-{}.sqlite",
        std::process::id()
    ))
}

fn sample_loop(name: &str) -> Loop {
    Loop {
        name: name.into(),
        repo_path: "/repo".into(),
        interval_seconds: 15,
        state: LoopState::Running,
        log_path: "/tmp/loop.log".into(),
        ledger_path: "/repo/.forge/ledgers/test.md".into(),
        tags: vec!["review".into()],
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Tests — mirrors Go TestLoopRepository_CreateGetUpdate
// ---------------------------------------------------------------------------

#[test]
fn create_get_update() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    let mut l = sample_loop("Smart Homer");
    repo.create(&mut l).expect("create");

    assert!(!l.id.is_empty(), "ID should be auto-generated");
    assert!(!l.short_id.is_empty(), "short_id should be auto-generated");
    assert!(!l.created_at.is_empty(), "created_at should be set");
    assert!(!l.updated_at.is_empty(), "updated_at should be set");

    // GetByName
    let fetched = repo.get_by_name("Smart Homer").expect("get_by_name");
    assert_eq!(fetched.repo_path, "/repo");
    assert_eq!(fetched.short_id, l.short_id);
    assert_eq!(fetched.state, LoopState::Running);
    assert_eq!(fetched.tags, vec!["review".to_string()]);

    // Update state
    let mut updated = fetched;
    updated.state = LoopState::Sleeping;
    repo.update(&mut updated).expect("update");

    let re_fetched = repo.get(&updated.id).expect("get after update");
    assert_eq!(re_fetched.state, LoopState::Sleeping);

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_by_short_id() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    let mut l = sample_loop("short-id-test");
    repo.create(&mut l).expect("create");

    let fetched = repo.get_by_short_id(&l.short_id).expect("get_by_short_id");
    assert_eq!(fetched.id, l.id);
    assert_eq!(fetched.name, "short-id-test");

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_empty_and_populated() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    let loops = repo.list().expect("list empty");
    assert!(loops.is_empty());

    let mut l1 = sample_loop("loop-a");
    let mut l2 = sample_loop("loop-b");
    repo.create(&mut l1).expect("create l1");
    repo.create(&mut l2).expect("create l2");

    let loops = repo.list().expect("list populated");
    assert_eq!(loops.len(), 2);
    // Ordered by created_at — both are very close but l1 was first.
    assert_eq!(loops[0].name, "loop-a");
    assert_eq!(loops[1].name, "loop-b");

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    let mut l = sample_loop("delete-me");
    repo.create(&mut l).expect("create");

    repo.delete(&l.id).expect("delete");

    match repo.get(&l.id) {
        Err(DbError::LoopNotFound) => {}
        other => panic!("expected LoopNotFound, got: {other:?}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_not_found() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    match repo.delete("nonexistent-id") {
        Err(DbError::LoopNotFound) => {}
        other => panic!("expected LoopNotFound, got: {other:?}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn duplicate_name_returns_already_exists() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    let mut l1 = sample_loop("dupe");
    repo.create(&mut l1).expect("create first");

    let mut l2 = sample_loop("dupe");
    match repo.create(&mut l2) {
        Err(DbError::LoopAlreadyExists) => {}
        other => panic!("expected LoopAlreadyExists, got: {other:?}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_not_found() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    let mut l = Loop {
        id: "nonexistent".into(),
        short_id: "abcdefgh".into(),
        name: "ghost".into(),
        repo_path: "/repo".into(),
        ..Default::default()
    };

    match repo.update(&mut l) {
        Err(DbError::LoopNotFound) => {}
        other => panic!("expected LoopNotFound, got: {other:?}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn metadata_roundtrip() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    let mut metadata = std::collections::HashMap::new();
    metadata.insert(
        "stop_config".into(),
        serde_json::json!({"mode": "quantitative"}),
    );

    let mut l = Loop {
        name: "meta-test".into(),
        repo_path: "/repo".into(),
        state: LoopState::Stopped,
        metadata: Some(metadata),
        ..Default::default()
    };
    repo.create(&mut l).expect("create");

    let fetched = repo.get(&l.id).expect("get");
    let meta = fetched.metadata.expect("metadata should be Some");
    assert_eq!(
        meta.get("stop_config").and_then(|v| v.get("mode")),
        Some(&serde_json::json!("quantitative"))
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn optional_fields_roundtrip() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    let mut l = Loop {
        name: "optional-test".into(),
        repo_path: "/repo".into(),
        last_run_at: Some("2024-01-01T00:00:00Z".into()),
        last_exit_code: Some(42),
        last_error: "something went wrong".into(),
        base_prompt_path: "/prompts/base.md".into(),
        base_prompt_msg: "hello there".into(),
        pool_id: String::new(),
        profile_id: String::new(),
        max_iterations: 10,
        max_runtime_seconds: 3600,
        ..Default::default()
    };
    repo.create(&mut l).expect("create");

    let fetched = repo.get(&l.id).expect("get");
    assert_eq!(fetched.last_run_at.as_deref(), Some("2024-01-01T00:00:00Z"));
    assert_eq!(fetched.last_exit_code, Some(42));
    assert_eq!(fetched.last_error, "something went wrong");
    assert_eq!(fetched.base_prompt_path, "/prompts/base.md");
    assert_eq!(fetched.base_prompt_msg, "hello there");
    assert_eq!(fetched.pool_id, "");
    assert_eq!(fetched.profile_id, "");
    assert_eq!(fetched.max_iterations, 10);
    assert_eq!(fetched.max_runtime_seconds, 3600);

    let _ = std::fs::remove_file(path);
}

#[test]
fn short_id_is_lowercased_on_create() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    let mut l = Loop {
        name: "upper-short-id".into(),
        repo_path: "/repo".into(),
        short_id: "ABCDEFGH".into(),
        ..Default::default()
    };
    repo.create(&mut l).expect("create");
    assert_eq!(l.short_id, "abcdefgh");

    let fetched = repo.get_by_short_id("abcdefgh").expect("get_by_short_id");
    assert_eq!(fetched.name, "upper-short-id");

    let _ = std::fs::remove_file(path);
}

#[test]
fn all_states_roundtrip() {
    let (db, path) = setup_db();
    let repo = LoopRepository::new(&db);

    let states = [
        LoopState::Running,
        LoopState::Sleeping,
        LoopState::Waiting,
        LoopState::Stopped,
        LoopState::Error,
    ];

    for (i, state) in states.iter().enumerate() {
        let mut l = Loop {
            name: format!("state-test-{i}"),
            repo_path: "/repo".into(),
            state: state.clone(),
            ..Default::default()
        };
        repo.create(&mut l).expect("create");

        let fetched = repo.get(&l.id).expect("get");
        assert_eq!(
            &fetched.state,
            state,
            "state mismatch for {}",
            state.as_str()
        );
    }

    let _ = std::fs::remove_file(path);
}
