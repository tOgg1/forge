#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Integration tests for loop_run_repository — mirrors Go TestLoopRunRepository_*.

use forge_db::loop_repository::{Loop, LoopRepository, LoopState};
use forge_db::loop_run_repository::{LoopRun, LoopRunRepository, LoopRunStatus};
use forge_db::profile_repository::{Profile, ProfileRepository};
use forge_db::{Config, Db, DbError};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(tag: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-loop-run-integ-{tag}-{nanos}-{}-{suffix}.sqlite",
        std::process::id(),
    ))
}

fn open_migrated(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let _ = std::fs::remove_file(&path);
    let mut db = Db::open(Config::new(&path)).expect("open db");
    db.migrate_up().expect("migrate");
    (db, path)
}

fn create_test_loop(db: &Db) -> Loop {
    let repo = LoopRepository::new(db);
    let mut l = Loop {
        name: format!("test-loop-{}", uuid::Uuid::new_v4()),
        repo_path: "/repo".to_string(),
        state: LoopState::Stopped,
        ..Loop::default()
    };
    repo.create(&mut l).expect("create loop");
    l
}

fn create_test_profile(db: &Db) -> Profile {
    let repo = ProfileRepository::new(db);
    let mut p = Profile {
        name: format!("pi-runner-{}", uuid::Uuid::new_v4()),
        harness: "pi".to_string(),
        command_template: "pi -p \"{prompt}\"".to_string(),
        max_concurrency: 1,
        prompt_mode: "path".to_string(),
        ..Profile::default()
    };
    repo.create(&mut p).expect("create profile");
    p
}

// Mirrors Go TestLoopRunRepository_CreateFinish
#[test]
fn create_and_finish_integration() {
    let (db, path) = open_migrated("create-finish-integ");
    let test_loop = create_test_loop(&db);
    let profile = create_test_profile(&db);
    let repo = LoopRunRepository::new(&db);

    let mut run = LoopRun {
        loop_id: test_loop.id.clone(),
        profile_id: profile.id.clone(),
        prompt_source: "base".to_string(),
        status: LoopRunStatus::Running,
        ..LoopRun::default()
    };
    repo.create(&mut run).expect("create run");
    assert!(!run.id.is_empty());
    assert!(!run.started_at.is_empty());

    // Finish with success
    run.status = LoopRunStatus::Success;
    run.exit_code = Some(0);
    run.output_tail = "ok".to_string();
    repo.finish(&mut run).expect("finish");

    let stored = repo.get(&run.id).expect("get");
    assert_eq!(stored.status, LoopRunStatus::Success);
    assert_eq!(stored.exit_code, Some(0));
    assert_eq!(stored.output_tail, "ok");
    assert!(stored.finished_at.is_some());

    let _ = std::fs::remove_file(path);
}

// Mirrors Go TestLoopRunRepository_CountByLoop
#[test]
fn count_by_loop_integration() {
    let (db, path) = open_migrated("count-integ");
    let repo = LoopRunRepository::new(&db);
    let loop_a = create_test_loop(&db);
    let loop_b = create_test_loop(&db);

    for _ in 0..3 {
        let mut run = LoopRun {
            loop_id: loop_a.id.clone(),
            prompt_source: "base".to_string(),
            status: LoopRunStatus::Running,
            ..LoopRun::default()
        };
        repo.create(&mut run).expect("create");
    }

    let mut run_b = LoopRun {
        loop_id: loop_b.id.clone(),
        prompt_source: "base".to_string(),
        status: LoopRunStatus::Running,
        ..LoopRun::default()
    };
    repo.create(&mut run_b).expect("create");

    assert_eq!(repo.count_by_loop(&loop_a.id).expect("count a"), 3);
    assert_eq!(repo.count_by_loop(&loop_b.id).expect("count b"), 1);

    let _ = std::fs::remove_file(path);
}

// Test list_by_loop ordering matches Go (DESC by started_at)
#[test]
fn list_by_loop_ordering_integration() {
    let (db, path) = open_migrated("list-order-integ");
    let repo = LoopRunRepository::new(&db);
    let test_loop = create_test_loop(&db);

    for i in 0..3 {
        let mut run = LoopRun {
            loop_id: test_loop.id.clone(),
            prompt_source: format!("source-{i}"),
            status: LoopRunStatus::Running,
            started_at: format!("2026-01-0{}T00:00:00Z", i + 1),
            ..LoopRun::default()
        };
        repo.create(&mut run).expect("create");
    }

    let runs = repo.list_by_loop(&test_loop.id).expect("list");
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].prompt_source, "source-2");
    assert_eq!(runs[1].prompt_source, "source-1");
    assert_eq!(runs[2].prompt_source, "source-0");

    let _ = std::fs::remove_file(path);
}

// Test count_running_by_profile only counts running
#[test]
fn count_running_by_profile_integration() {
    let (db, path) = open_migrated("count-running-integ");
    let repo = LoopRunRepository::new(&db);
    let test_loop = create_test_loop(&db);
    let profile = create_test_profile(&db);

    // 2 running
    for _ in 0..2 {
        let mut run = LoopRun {
            loop_id: test_loop.id.clone(),
            profile_id: profile.id.clone(),
            status: LoopRunStatus::Running,
            ..LoopRun::default()
        };
        repo.create(&mut run).expect("create");
    }

    // 1 finished
    let mut finished = LoopRun {
        loop_id: test_loop.id.clone(),
        profile_id: profile.id.clone(),
        status: LoopRunStatus::Success,
        started_at: "2026-01-01T00:00:00Z".to_string(),
        finished_at: Some("2026-01-01T00:01:00Z".to_string()),
        ..LoopRun::default()
    };
    repo.create(&mut finished).expect("create finished");

    let count = repo.count_running_by_profile(&profile.id).expect("count");
    assert_eq!(count, 2, "only running runs counted");

    let _ = std::fs::remove_file(path);
}

// Test get returns LoopRunNotFound for nonexistent
#[test]
fn get_not_found_integration() {
    let (db, path) = open_migrated("get-404-integ");
    let repo = LoopRunRepository::new(&db);
    let err = repo.get("nonexistent");
    assert!(matches!(err, Err(DbError::LoopRunNotFound)));
    let _ = std::fs::remove_file(path);
}

// Test finish returns LoopRunNotFound for nonexistent
#[test]
fn finish_not_found_integration() {
    let (db, path) = open_migrated("finish-404-integ");
    let repo = LoopRunRepository::new(&db);
    let mut run = LoopRun {
        id: "nonexistent".to_string(),
        status: LoopRunStatus::Error,
        ..LoopRun::default()
    };
    let err = repo.finish(&mut run);
    assert!(matches!(err, Err(DbError::LoopRunNotFound)));
    let _ = std::fs::remove_file(path);
}

// Test FK cascade: deleting a loop cascades to loop_runs
#[test]
fn loop_delete_cascades_to_runs() {
    let (db, path) = open_migrated("cascade-integ");
    let loop_repo = LoopRepository::new(&db);
    let run_repo = LoopRunRepository::new(&db);

    let mut l = Loop {
        name: "cascade-test".to_string(),
        repo_path: "/repo".to_string(),
        state: LoopState::Stopped,
        ..Loop::default()
    };
    loop_repo.create(&mut l).expect("create loop");

    for _ in 0..3 {
        let mut run = LoopRun {
            loop_id: l.id.clone(),
            status: LoopRunStatus::Running,
            ..LoopRun::default()
        };
        run_repo.create(&mut run).expect("create run");
    }

    assert_eq!(run_repo.count_by_loop(&l.id).expect("count"), 3);

    loop_repo.delete(&l.id).expect("delete loop");

    assert_eq!(
        run_repo.count_by_loop(&l.id).expect("count after delete"),
        0,
        "runs should be cascade-deleted"
    );

    let _ = std::fs::remove_file(path);
}

// Test metadata JSON roundtrip through integration path
#[test]
fn metadata_roundtrip_integration() {
    let (db, path) = open_migrated("metadata-integ");
    let repo = LoopRunRepository::new(&db);
    let test_loop = create_test_loop(&db);

    let mut meta = std::collections::HashMap::new();
    meta.insert("pid".to_string(), serde_json::json!(42));
    meta.insert("host".to_string(), serde_json::json!("worker-1"));

    let mut run = LoopRun {
        loop_id: test_loop.id.clone(),
        status: LoopRunStatus::Running,
        metadata: Some(meta),
        ..LoopRun::default()
    };
    repo.create(&mut run).expect("create");

    let stored = repo.get(&run.id).expect("get");
    let m = stored.metadata.as_ref().expect("metadata should be Some");
    assert_eq!(m.get("pid"), Some(&serde_json::json!(42)));
    assert_eq!(m.get("host"), Some(&serde_json::json!("worker-1")));

    let _ = std::fs::remove_file(path);
}
