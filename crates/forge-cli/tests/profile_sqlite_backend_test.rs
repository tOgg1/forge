use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn profile_dispatch_persists_to_sqlite_and_lists() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, dir) = setup_db("profile_dispatch_persists_to_sqlite_and_lists");
    let old_db = std::env::var_os("FORGE_DATABASE_PATH");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);

    migrate(&db_path);

    let (code, stdout, stderr) = run(&["profile", "ls", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    let listed: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("parse json: {e}"));
    assert_eq!(listed, serde_json::json!([]));

    let (code, stdout, stderr) = run(&["profile", "add", "codex", "--name", "alpha", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    let created: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("parse json: {e}"));
    assert_eq!(created["name"], "alpha");
    assert_eq!(created["harness"], "codex");
    assert!(created["id"].as_str().is_some());

    let (code, stdout, stderr) = run(&["profile", "ls", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    let listed: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("parse json: {e}"));
    let items = listed
        .as_array()
        .unwrap_or_else(|| panic!("expected array, got: {listed:?}"));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "alpha");

    let (code, stdout, stderr) = run(&[
        "profile",
        "cooldown",
        "set",
        "alpha",
        "--until",
        "2026-02-10T00:00:00Z",
        "--json",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    let updated: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("parse json: {e}"));
    assert_eq!(updated["cooldown_until"], "2026-02-10T00:00:00Z");

    let (code, stdout, stderr) = run(&["profile", "cooldown", "clear", "alpha", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    let updated: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("parse json: {e}"));
    assert!(updated.get("cooldown_until").is_none());

    match old_db {
        Some(value) => std::env::set_var("FORGE_DATABASE_PATH", value),
        None => std::env::remove_var("FORGE_DATABASE_PATH"),
    }
    drop(dir);
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

fn migrate(db_path: &PathBuf) {
    let mut db = forge_db::Db::open(forge_db::Config::new(db_path))
        .unwrap_or_else(|e| panic!("open db {}: {e}", db_path.display()));
    db.migrate_up().unwrap_or_else(|e| panic!("migrate: {e}"));
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
