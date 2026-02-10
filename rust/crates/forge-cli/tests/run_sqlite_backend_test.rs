use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn run_dispatch_uses_sqlite_backend_and_records_iteration() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, dir) = setup_db("run_dispatch_uses_sqlite_backend_and_records_iteration");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);
    std::env::set_var("FORGE_DATA_DIR", dir.path.join("data"));

    let loop_id = {
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db {}: {err}", db_path.display()));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);
        let repo_path = dir.path.join("repo");
        std::fs::create_dir_all(&repo_path)
            .unwrap_or_else(|err| panic!("mkdir {}: {err}", repo_path.display()));

        let mut profile = forge_db::profile_repository::Profile {
            name: "runner-profile".to_string(),
            harness: "codex".to_string(),
            prompt_mode: "env".to_string(),
            command_template: "printf 'run ok\\n'".to_string(),
            ..Default::default()
        };
        profile_repo
            .create(&mut profile)
            .unwrap_or_else(|err| panic!("create profile: {err}"));

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "oracle-loop".to_string(),
            repo_path: repo_path.to_string_lossy().into_owned(),
            profile_id: profile.id.clone(),
            base_prompt_msg: "hello".to_string(),
            max_iterations: 1,
            state: forge_db::loop_repository::LoopState::Error,
            last_error: "previous failure".to_string(),
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));
        loop_entry.id
    };

    let (code, stdout, stderr) = run(&["run", "oracle-loop"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());

    let db = forge_db::Db::open(forge_db::Config::new(&db_path))
        .unwrap_or_else(|err| panic!("reopen db {}: {err}", db_path.display()));
    let run_repo = forge_db::loop_run_repository::LoopRunRepository::new(&db);
    let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);

    let runs = run_repo
        .list_by_loop(&loop_id)
        .unwrap_or_else(|err| panic!("list runs: {err}"));
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0].status,
        forge_db::loop_run_repository::LoopRunStatus::Success
    );
    assert_eq!(runs[0].exit_code, Some(0));
    assert!(runs[0].finished_at.is_some());

    let loop_entry = loop_repo
        .get(&loop_id)
        .unwrap_or_else(|err| panic!("get loop: {err}"));
    assert_eq!(
        loop_entry.state,
        forge_db::loop_repository::LoopState::Stopped
    );
    assert_eq!(loop_entry.last_exit_code, Some(0));
    assert!(loop_entry.last_run_at.is_some());
    assert!(loop_entry.last_error.is_empty());
}

#[test]
fn run_dispatch_profile_selection_error_sets_loop_error_state() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, dir) = setup_db("run_dispatch_profile_selection_error_sets_loop_error_state");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);
    std::env::set_var("FORGE_DATA_DIR", dir.path.join("data"));

    let loop_id = {
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db {}: {err}", db_path.display()));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let repo_path = dir.path.join("repo");
        std::fs::create_dir_all(&repo_path)
            .unwrap_or_else(|err| panic!("mkdir {}: {err}", repo_path.display()));

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "orphan-loop".to_string(),
            repo_path: repo_path.to_string_lossy().into_owned(),
            base_prompt_msg: "hello".to_string(),
            state: forge_db::loop_repository::LoopState::Stopped,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));
        loop_entry.id
    };

    let (code, stdout, stderr) = run(&["run", "orphan-loop"]);
    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(
        stderr.contains("loop run failed: pool unavailable"),
        "stderr: {stderr}"
    );

    let db = forge_db::Db::open(forge_db::Config::new(&db_path))
        .unwrap_or_else(|err| panic!("reopen db {}: {err}", db_path.display()));
    let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
    let loop_entry = loop_repo
        .get(&loop_id)
        .unwrap_or_else(|err| panic!("get loop: {err}"));
    assert_eq!(
        loop_entry.state,
        forge_db::loop_repository::LoopState::Error
    );
    assert!(
        loop_entry.last_error.contains("pool unavailable"),
        "last_error: {}",
        loop_entry.last_error
    );
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

fn setup_db(test_name: &str) -> (PathBuf, TempDir) {
    let dir = TempDir::new(test_name);
    (dir.path.join("forge.db"), dir)
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
        std::fs::create_dir_all(&path)
            .unwrap_or_else(|err| panic!("mkdir {}: {err}", path.display()));
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
