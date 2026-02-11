//! Loop work state repository integration tests — Go parity coverage.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::loop_repository::{Loop, LoopRepository, LoopState};
use forge_db::loop_work_state_repository::{LoopWorkState, LoopWorkStateRepository};
use forge_db::{Config, Db, DbError};

fn temp_db_path(prefix: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-work-{prefix}-{nanos}-{}-{suffix}.sqlite",
        std::process::id(),
    ))
}

fn setup_db(tag: &str) -> (Db, PathBuf) {
    let path = temp_db_path(tag);
    let mut db = match Db::open(Config::new(&path)) {
        Ok(db) => db,
        Err(err) => panic!("open db: {err}"),
    };
    if let Err(err) = db.migrate_up() {
        panic!("migrate_up: {err}");
    }
    (db, path)
}

fn make_loop(name: &str) -> Loop {
    Loop {
        name: name.to_string(),
        repo_path: std::env::temp_dir()
            .join("forge-db-work-loop")
            .display()
            .to_string(),
        base_prompt_msg: "base".to_string(),
        interval_seconds: 1,
        state: LoopState::Stopped,
        ..Default::default()
    }
}

#[test]
fn set_current_clears_previous() {
    let (mut db, path) = setup_db("set-current-clears");
    let loop_repo = LoopRepository::new(&db);

    let mut l = make_loop("loop-work-test");
    match loop_repo.create(&mut l) {
        Ok(()) => {}
        Err(e) => panic!("create loop: {e}"),
    }

    let mut repo = LoopWorkStateRepository::new(&mut db);

    let mut s1 = LoopWorkState {
        loop_id: l.id.clone(),
        agent_id: "a".to_string(),
        task_id: "sv-1".to_string(),
        status: "blocked".to_string(),
        detail: "waiting".to_string(),
        loop_iteration: 3,
        ..Default::default()
    };
    match repo.set_current(&mut s1) {
        Ok(()) => {}
        Err(e) => panic!("set_current s1: {e}"),
    }

    let cur = match repo.get_current(&l.id) {
        Ok(s) => s,
        Err(e) => panic!("get_current: {e}"),
    };
    assert_eq!(cur.task_id, "sv-1");
    assert!(cur.is_current);

    let mut s2 = LoopWorkState {
        loop_id: l.id.clone(),
        agent_id: "a".to_string(),
        task_id: "sv-2".to_string(),
        status: "in_progress".to_string(),
        loop_iteration: 4,
        ..Default::default()
    };
    match repo.set_current(&mut s2) {
        Ok(()) => {}
        Err(e) => panic!("set_current s2: {e}"),
    }

    let cur = match repo.get_current(&l.id) {
        Ok(s) => s,
        Err(e) => panic!("get_current after s2: {e}"),
    };
    assert_eq!(cur.task_id, "sv-2");
    assert!(cur.is_current);

    let items = match repo.list_by_loop(&l.id, 0) {
        Ok(v) => v,
        Err(e) => panic!("list_by_loop: {e}"),
    };
    assert_eq!(items.len(), 2);
    assert!(items[0].is_current);
    assert!(!items[1].is_current);

    let _ = std::fs::remove_file(path);
}

#[test]
fn set_current_defaults_status_and_trims() {
    let (mut db, path) = setup_db("defaults-trims");
    let loop_repo = LoopRepository::new(&db);

    let mut l = make_loop("trim-loop");
    match loop_repo.create(&mut l) {
        Ok(()) => {}
        Err(e) => panic!("create loop: {e}"),
    }

    let mut repo = LoopWorkStateRepository::new(&mut db);

    let mut s = LoopWorkState {
        loop_id: format!("  {}  ", l.id),
        agent_id: "  agent-1 ".to_string(),
        task_id: "  task-1 ".to_string(),
        status: "  ".to_string(),
        ..Default::default()
    };

    match repo.set_current(&mut s) {
        Ok(()) => {}
        Err(e) => panic!("set_current: {e}"),
    }

    let cur = match repo.get_current(&l.id) {
        Ok(v) => v,
        Err(e) => panic!("get_current: {e}"),
    };
    assert_eq!(cur.agent_id, "agent-1");
    assert_eq!(cur.task_id, "task-1");
    assert_eq!(cur.status, "in_progress");

    let _ = std::fs::remove_file(path);
}

#[test]
fn get_current_not_found() {
    let (mut db, path) = setup_db("not-found");
    let mut repo = LoopWorkStateRepository::new(&mut db);
    match repo.get_current("missing-loop") {
        Ok(_) => panic!("expected not found"),
        Err(DbError::LoopWorkStateNotFound) => {}
        Err(e) => panic!("unexpected error: {e}"),
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn list_by_loop_default_limit_is_200() {
    let (mut db, path) = setup_db("default-limit");
    let loop_repo = LoopRepository::new(&db);

    let mut l = make_loop("limit-loop");
    match loop_repo.create(&mut l) {
        Ok(()) => {}
        Err(e) => panic!("create loop: {e}"),
    }

    let conn = db.conn();
    for i in 0..205 {
        let id = format!("ws-{i}");
        let task_id = format!("task-{i}");
        if let Err(e) = conn.execute(
            "INSERT INTO loop_work_state (id, loop_id, agent_id, task_id, status) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, l.id, "agent", task_id, "in_progress"],
        ) {
            panic!("insert {i}: {e}");
        }
    }

    let mut repo = LoopWorkStateRepository::new(&mut db);
    let items = match repo.list_by_loop(&l.id, 0) {
        Ok(v) => v,
        Err(e) => panic!("list_by_loop: {e}"),
    };
    assert_eq!(items.len(), 200);

    let _ = std::fs::remove_file(path);
}
