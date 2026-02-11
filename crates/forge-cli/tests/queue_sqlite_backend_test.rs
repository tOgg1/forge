use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn queue_dispatch_uses_sqlite_backend_and_supports_global_json_flag() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, _dir) =
        setup_db("queue_dispatch_uses_sqlite_backend_and_supports_global_json_flag");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);

    seed_db(&db_path);

    // Global --json should work (queue parser expects flags after positionals).
    let (code, stdout, stderr) = run(&["--json", "queue", "ls", "oracle-loop"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(
        stdout,
        include_str!("golden/queue/ls_pending.json"),
        "stdout mismatch"
    );
}

#[test]
fn queue_dispatch_supports_clear_rm_move_against_sqlite() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, _dir) = setup_db("queue_dispatch_supports_clear_rm_move_against_sqlite");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);

    seed_db(&db_path);

    let (code, stdout, stderr) = run(&["queue", "ls", "oracle-loop", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(stdout, include_str!("golden/queue/ls_pending.json"));

    let (code, stdout, stderr) = run(&[
        "queue",
        "move",
        "oracle-loop",
        "q3",
        "--to",
        "front",
        "--json",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(stdout, include_str!("golden/queue/move.json"));

    let (code, stdout, stderr) = run(&["queue", "rm", "oracle-loop", "q1", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(stdout, include_str!("golden/queue/rm.json"));

    let (code, stdout, stderr) = run(&["queue", "clear", "oracle-loop", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(stdout, "{\n  \"cleared\": 1\n}\n");
}

fn run(args: &[&str]) -> (i32, String, String) {
    let argv: Vec<String> = args.iter().map(|v| (*v).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = forge_cli::run_with_args(&argv, &mut stdout, &mut stderr);
    (
        code,
        String::from_utf8(stdout).unwrap_or_default(),
        String::from_utf8(stderr).unwrap_or_default(),
    )
}

fn setup_db(test_name: &str) -> (PathBuf, TempDir) {
    let dir = TempDir::new(test_name);
    (dir.path.join("forge.db"), dir)
}

fn seed_db(db_path: &PathBuf) {
    let mut db = forge_db::Db::open(forge_db::Config::new(db_path))
        .unwrap_or_else(|e| panic!("open db {}: {e}", db_path.display()));
    db.migrate_up().unwrap_or_else(|e| panic!("migrate: {e}"));

    let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
    let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);

    let mut lp = forge_db::loop_repository::Loop {
        name: "oracle-loop".to_string(),
        repo_path: "/repo".to_string(),
        interval_seconds: 10,
        ..Default::default()
    };
    loop_repo
        .create(&mut lp)
        .unwrap_or_else(|e| panic!("create loop: {e}"));

    let mut items = vec![
        forge_db::loop_queue_repository::LoopQueueItem {
            id: "q1".to_string(),
            item_type: "message_append".to_string(),
            payload: "{\"text\":\"hello\"}".to_string(),
            status: "pending".to_string(),
            ..Default::default()
        },
        forge_db::loop_queue_repository::LoopQueueItem {
            id: "q2".to_string(),
            item_type: "stop_graceful".to_string(),
            payload: "{\"reason\":\"done\"}".to_string(),
            status: "completed".to_string(),
            ..Default::default()
        },
        forge_db::loop_queue_repository::LoopQueueItem {
            id: "q3".to_string(),
            item_type: "kill_now".to_string(),
            payload: "{\"reason\":\"boom\"}".to_string(),
            status: "pending".to_string(),
            ..Default::default()
        },
    ];
    queue_repo
        .enqueue(&lp.id, &mut items)
        .unwrap_or_else(|e| panic!("enqueue: {e}"));

    // Make outputs deterministic for golden comparisons.
    let conn = db.conn();
    conn.execute(
        "UPDATE loop_queue_items SET created_at = ?1 WHERE id = ?2",
        rusqlite::params!["2025-01-01T00:00:00Z", "q1"],
    )
    .unwrap_or_else(|e| panic!("update created_at q1: {e}"));
    conn.execute(
        "UPDATE loop_queue_items SET created_at = ?1 WHERE id = ?2",
        rusqlite::params!["2025-01-01T00:00:01Z", "q2"],
    )
    .unwrap_or_else(|e| panic!("update created_at q2: {e}"));
    conn.execute(
        "UPDATE loop_queue_items SET created_at = ?1 WHERE id = ?2",
        rusqlite::params!["2025-01-01T00:00:02Z", "q3"],
    )
    .unwrap_or_else(|e| panic!("update created_at q3: {e}"));
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let mut path = std::env::temp_dir();
        let uniq = format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        path.push(uniq);
        std::fs::create_dir_all(&path).unwrap_or_else(|e| panic!("mkdir {}: {e}", path.display()));
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
