use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn seq_dispatch_uses_filesystem_backend_for_storage_and_run() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let temp = TempDir::new("seq-fs-backend");
    let repo = temp.path.join("repo");
    let home = temp.path.join("home");
    std::fs::create_dir_all(&repo).unwrap_or_else(|e| panic!("mkdir repo: {e}"));
    std::fs::create_dir_all(&home).unwrap_or_else(|e| panic!("mkdir home: {e}"));

    // Ensure git-root discovery finds repo root like Go's getGitRoot.
    std::fs::create_dir_all(repo.join(".git")).unwrap_or_else(|e| panic!("mkdir .git: {e}"));

    seed_sequences(&repo);
    let db_path = temp.path.join("forge.db");
    seed_db(&db_path, &repo);

    std::env::set_var("FORGE_DATABASE_PATH", &db_path);
    std::env::set_var("HOME", &home);
    std::env::set_var("EDITOR", "true");

    with_working_dir(&repo, || {
        let (code, stdout, stderr) = run(&["seq", "ls", "--json"]);
        assert_eq!(code, 0, "stderr: {stderr}");
        assert!(stderr.is_empty());
        let list: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
        let items = list
            .as_array()
            .unwrap_or_else(|| panic!("expected array: {list}"));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["Name"], "oracle-seq");
        assert!(items[0]["Source"]
            .as_str()
            .is_some_and(|s| s.ends_with(".forge/sequences/oracle-seq.yaml")));

        let (code, stdout, stderr) = run(&["seq", "show", "oracle-seq", "--json"]);
        assert_eq!(code, 0, "stderr: {stderr}");
        assert!(stderr.is_empty());
        let show: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
        assert_eq!(show["Name"], "oracle-seq");

        let (code, stdout, stderr) = run(&[
            "seq",
            "run",
            "oracle-seq",
            "--agent",
            "agent_12345678",
            "--json",
        ]);
        assert_eq!(code, 0, "stderr: {stderr}");
        assert!(stderr.is_empty());
        let run_value: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
        assert_eq!(run_value["sequence"], "oracle-seq");
        assert_eq!(run_value["agent_id"], "agent_12345678");
        let item_ids = run_value["item_ids"]
            .as_array()
            .unwrap_or_else(|| panic!("missing item_ids: {run_value}"));
        assert_eq!(item_ids.len(), 3);

        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|e| panic!("open db {}: {e}", db_path.display()));
        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(1) FROM queue_items WHERE agent_id = ?1",
                rusqlite::params!["agent_12345678"],
                |row| row.get(0),
            )
            .unwrap_or_else(|e| panic!("count queued items: {e}"));
        assert_eq!(count, 3);

        let (code, stdout, stderr) = run(&["seq", "add", "new-seq", "--json"]);
        assert_eq!(code, 0, "stderr: {stderr}");
        assert!(stderr.is_empty());
        let added: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
        let added_path = added["path"]
            .as_str()
            .unwrap_or_else(|| panic!("missing path in add output: {added}"));
        assert!(Path::new(added_path).exists());

        let (code, _stdout, stderr) = run(&["seq", "edit", "new-seq", "--json"]);
        assert_eq!(code, 0, "stderr: {stderr}");
        assert!(stderr.is_empty());

        let (code, stdout, stderr) = run(&["seq", "delete", "new-seq", "--json"]);
        assert_eq!(code, 0, "stderr: {stderr}");
        assert!(stderr.is_empty());
        let deleted: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("parse json: {e}\n{stdout}"));
        assert_eq!(deleted["deleted"], "new-seq");
        assert!(!Path::new(added_path).exists());
    });
}

fn run(args: &[&str]) -> (i32, String, String) {
    let argv: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = forge_cli::run_with_args(&argv, &mut stdout, &mut stderr);
    (
        code,
        String::from_utf8(stdout).unwrap_or_default(),
        String::from_utf8(stderr).unwrap_or_default(),
    )
}

fn seed_sequences(repo: &Path) {
    let project_dir = repo.join(".forge").join("sequences");
    std::fs::create_dir_all(&project_dir)
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", project_dir.display()));

    let oracle = r#"
name: oracle-seq
description: Describe this sequence
steps:
  - type: message
    content: Step 1.
  - type: pause
    duration: 3s
    reason: Wait
  - type: conditional
    when: idle
    message: Step 3 when idle.
"#;
    std::fs::write(project_dir.join("oracle-seq.yaml"), oracle.trim_start())
        .unwrap_or_else(|e| panic!("write oracle-seq.yaml: {e}"));
}

fn seed_db(db_path: &Path, repo: &Path) {
    let mut db = forge_db::Db::open(forge_db::Config::new(db_path))
        .unwrap_or_else(|e| panic!("open db {}: {e}", db_path.display()));
    db.migrate_up().unwrap_or_else(|e| panic!("migrate: {e}"));
    let conn = db.conn();
    conn.execute(
        "INSERT INTO nodes (id, name, is_local, status) VALUES ('node_1', 'local', 1, 'online')",
        [],
    )
    .unwrap_or_else(|e| panic!("insert node: {e}"));
    conn.execute(
        "INSERT INTO workspaces (id, name, node_id, repo_path, tmux_session, status)
         VALUES ('ws_1', 'alpha', 'node_1', ?1, 'alpha-session', 'active')",
        rusqlite::params![repo.to_string_lossy().to_string()],
    )
    .unwrap_or_else(|e| panic!("insert workspace: {e}"));
    conn.execute(
        "INSERT INTO agents (
            id, workspace_id, type, tmux_pane, state, state_confidence
        ) VALUES (
            'agent_12345678', 'ws_1', 'codex', 'alpha-session:1.1', 'idle', 'high'
        )",
        [],
    )
    .unwrap_or_else(|e| panic!("insert agent: {e}"));
}

fn with_working_dir<F>(dir: &Path, run: F)
where
    F: FnOnce(),
{
    let original =
        std::env::current_dir().unwrap_or_else(|e| panic!("failed to capture current dir: {e}"));
    std::env::set_current_dir(dir).unwrap_or_else(|e| panic!("set current dir: {e}"));
    run();
    std::env::set_current_dir(&original).unwrap_or_else(|e| panic!("restore current dir: {e}"));
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let mut path = std::env::temp_dir();
        let unique = format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        path.push(unique);
        std::fs::create_dir_all(&path).unwrap_or_else(|e| panic!("mkdir {}: {e}", path.display()));
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
