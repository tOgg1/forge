use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn logs_dispatch_reads_filesystem_logs_via_sqlite_loop_lookup() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, dir) = setup_db("logs_dispatch_reads_filesystem_logs_via_sqlite_loop_lookup");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);
    std::env::set_var("FORGE_DATA_DIR", &dir.path);

    seed_loop(&db_path, "oracle-loop", "/repo");

    let log_path = forge_cli::logs::default_log_path(
        dir.path.to_string_lossy().as_ref(),
        "oracle-loop",
        "placeholder",
    );
    ensure_parent_dir(&log_path);
    std::fs::write(
        &log_path,
        "[2026-01-01T00:00:00Z] one\n[2026-01-01T00:00:01Z] two\n",
    )
    .unwrap_or_else(|e| panic!("write log {}: {e}", log_path));

    let (code, stdout, stderr) = run(&["logs", "oracle-loop", "--lines", "1"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(stdout, "==> oracle-loop <==\n[2026-01-01T00:00:01Z] two\n");
}

#[test]
fn logs_dispatch_supports_since_and_follow_flags_on_sqlite_backend() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, dir) =
        setup_db("logs_dispatch_supports_since_and_follow_flags_on_sqlite_backend");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);
    std::env::set_var("FORGE_DATA_DIR", &dir.path);

    seed_loop(&db_path, "oracle-loop", "/repo");

    let log_path = forge_cli::logs::default_log_path(
        dir.path.to_string_lossy().as_ref(),
        "oracle-loop",
        "placeholder",
    );
    ensure_parent_dir(&log_path);
    std::fs::write(
        &log_path,
        "[2026-01-01T00:00:00Z] old\n[2026-01-01T00:00:01Z] keep\n[2026-01-01T00:00:02Z] newest\n",
    )
    .unwrap_or_else(|e| panic!("write log {}: {e}", log_path));

    let (code, stdout, stderr) = run(&["logs", "oracle-loop", "--since", "2026-01-01T00:00:01Z"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(
        stdout,
        "==> oracle-loop <==\n[2026-01-01T00:00:01Z] keep\n[2026-01-01T00:00:02Z] newest\n"
    );

    std::env::set_var("FORGE_LOGS_FOLLOW_ONCE", "1");
    let (code, stdout, stderr) = run(&["logs", "oracle-loop", "--follow", "--lines", "1"]);
    std::env::remove_var("FORGE_LOGS_FOLLOW_ONCE");
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert_eq!(
        stdout,
        "==> oracle-loop <==\n[2026-01-01T00:00:02Z] newest\n"
    );
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

fn seed_loop(db_path: &PathBuf, name: &str, repo_path: &str) {
    let mut db = forge_db::Db::open(forge_db::Config::new(db_path))
        .unwrap_or_else(|e| panic!("open db {}: {e}", db_path.display()));
    db.migrate_up().unwrap_or_else(|e| panic!("migrate: {e}"));

    let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
    let mut lp = forge_db::loop_repository::Loop {
        name: name.to_string(),
        repo_path: repo_path.to_string(),
        interval_seconds: 10,
        ..Default::default()
    };
    loop_repo
        .create(&mut lp)
        .unwrap_or_else(|e| panic!("create loop: {e}"));
}

fn ensure_parent_dir(path: &str) {
    let parent = PathBuf::from(path)
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("missing parent for {path}"));
    std::fs::create_dir_all(&parent).unwrap_or_else(|e| panic!("mkdir {}: {e}", parent.display()));
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
