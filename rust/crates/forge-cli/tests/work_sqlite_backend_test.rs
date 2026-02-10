use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn work_dispatch_supports_set_current_ls_and_clear_against_sqlite() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, _dir) =
        setup_db("work_dispatch_supports_set_current_ls_and_clear_against_sqlite");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);
    std::env::set_var("FORGE_LOOP_NAME", "oracle-loop");
    std::env::remove_var("FORGE_LOOP_ID");
    std::env::set_var("FMAIL_AGENT", "oracle-agent");

    let loop_id = seed_db(&db_path);

    let (code, stdout, stderr) = run(&[
        "work",
        "set",
        "sv-123",
        "--status",
        "in_progress",
        "--detail",
        "ship work backend",
        "--json",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    let set_value: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
    assert_eq!(set_value["loop_id"], loop_id);
    assert_eq!(set_value["agent_id"], "oracle-agent");
    assert_eq!(set_value["task_id"], "sv-123");
    assert_eq!(set_value["status"], "in_progress");
    assert_eq!(set_value["detail"], "ship work backend");
    assert_eq!(set_value["is_current"], true);

    let (code, stdout, stderr) = run(&["work", "current", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    let current_value: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
    assert_eq!(current_value["task_id"], "sv-123");
    assert_eq!(current_value["is_current"], true);

    let (code, stdout, stderr) = run(&["work", "ls", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    let list_value: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
    let items = list_value
        .as_array()
        .unwrap_or_else(|| panic!("expected array: {list_value}"));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["task_id"], "sv-123");
    assert_eq!(items[0]["is_current"], true);

    let (code, stdout, stderr) = run(&["work", "clear", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    let clear_value: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
    assert_eq!(clear_value["loop"], "oracle-loop");
    assert_eq!(clear_value["ok"], true);

    let (code, stdout, stderr) = run(&["work", "current", "--json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    let none_value: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
    assert_eq!(none_value["current"], serde_json::Value::Null);
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

fn seed_db(db_path: &PathBuf) -> String {
    let mut db = forge_db::Db::open(forge_db::Config::new(db_path))
        .unwrap_or_else(|e| panic!("open db {}: {e}", db_path.display()));
    db.migrate_up().unwrap_or_else(|e| panic!("migrate: {e}"));

    let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
    let mut lp = forge_db::loop_repository::Loop {
        name: "oracle-loop".to_string(),
        repo_path: "/repo".to_string(),
        interval_seconds: 10,
        ..Default::default()
    };
    loop_repo
        .create(&mut lp)
        .unwrap_or_else(|e| panic!("create loop: {e}"));

    lp.id
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
