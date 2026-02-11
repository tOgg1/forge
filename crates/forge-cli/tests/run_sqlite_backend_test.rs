use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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

#[test]
fn run_dispatch_quantitative_stop_before_run_short_circuits_iteration() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, dir) = setup_db("run_dispatch_quantitative_stop_before_run");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);
    std::env::set_var("FORGE_DATA_DIR", dir.path.join("data"));

    let repo_path = dir.path.join("repo");
    std::fs::create_dir_all(&repo_path)
        .unwrap_or_else(|err| panic!("mkdir {}: {err}", repo_path.display()));

    let loop_id = {
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db {}: {err}", db_path.display()));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let mut profile = forge_db::profile_repository::Profile {
            name: "quant-profile".to_string(),
            harness: "codex".to_string(),
            prompt_mode: "env".to_string(),
            command_template: "printf 'should-not-run\\n' >> ran.txt".to_string(),
            ..Default::default()
        };
        profile_repo
            .create(&mut profile)
            .unwrap_or_else(|err| panic!("create profile: {err}"));

        let stop_config = json!({
            "quant": {
                "cmd": "printf 'PASS\\n'",
                "every_n": 1,
                "when": "before",
                "decision": "stop",
                "exit_codes": [0],
                "exit_invert": false,
                "stdout_mode": "nonempty",
                "stderr_mode": "any",
                "stdout_regex": "PASS",
                "stderr_regex": "",
                "timeout_seconds": 2
            }
        });
        let mut metadata = HashMap::new();
        metadata.insert("stop_config".to_string(), stop_config);

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "quant-before-loop".to_string(),
            repo_path: repo_path.to_string_lossy().into_owned(),
            profile_id: profile.id.clone(),
            base_prompt_msg: "hello".to_string(),
            max_iterations: 5,
            metadata: Some(metadata),
            state: forge_db::loop_repository::LoopState::Stopped,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));
        loop_entry.id
    };

    let (code, stdout, stderr) = run(&["run", "quant-before-loop"]);
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
    assert_eq!(runs.len(), 0, "before-run quant stop should skip main run");

    let loop_entry = loop_repo
        .get(&loop_id)
        .unwrap_or_else(|err| panic!("get loop: {err}"));
    assert_eq!(
        loop_entry.state,
        forge_db::loop_repository::LoopState::Stopped
    );
    assert!(
        loop_entry
            .last_error
            .contains("quantitative stop matched (before-run)"),
        "last_error: {}",
        loop_entry.last_error
    );
    assert!(
        !repo_path.join("ran.txt").exists(),
        "main profile command should not have executed"
    );
}

#[test]
fn run_dispatch_qualitative_stop_after_run_stops_loop() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, dir) = setup_db("run_dispatch_qualitative_stop_after_run");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);
    std::env::set_var("FORGE_DATA_DIR", dir.path.join("data"));

    let repo_path = dir.path.join("repo");
    std::fs::create_dir_all(&repo_path)
        .unwrap_or_else(|err| panic!("mkdir {}: {err}", repo_path.display()));

    let loop_id = {
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db {}: {err}", db_path.display()));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let mut profile = forge_db::profile_repository::Profile {
            name: "qual-profile".to_string(),
            harness: "codex".to_string(),
            prompt_mode: "env".to_string(),
            command_template: "printf 'ran\\n' >> ran.txt".to_string(),
            ..Default::default()
        };
        profile_repo
            .create(&mut profile)
            .unwrap_or_else(|err| panic!("create profile: {err}"));

        let stop_config = json!({
            "qual": {
                "every_n": 1,
                "prompt": "0 stop",
                "is_prompt_path": false,
                "on_invalid": "continue"
            }
        });
        let mut metadata = HashMap::new();
        metadata.insert("stop_config".to_string(), stop_config);

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "qual-after-loop".to_string(),
            repo_path: repo_path.to_string_lossy().into_owned(),
            profile_id: profile.id.clone(),
            base_prompt_msg: "hello".to_string(),
            max_iterations: 5,
            metadata: Some(metadata),
            state: forge_db::loop_repository::LoopState::Stopped,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));
        loop_entry.id
    };

    let (code, stdout, stderr) = run(&["loop", "run", "qual-after-loop"]);
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
    assert_eq!(
        runs.len(),
        1,
        "main run should execute once before qual stop"
    );

    let loop_entry = loop_repo
        .get(&loop_id)
        .unwrap_or_else(|err| panic!("get loop: {err}"));
    assert_eq!(
        loop_entry.state,
        forge_db::loop_repository::LoopState::Stopped
    );
    assert!(
        loop_entry.last_error.contains("qualitative stop matched"),
        "last_error: {}",
        loop_entry.last_error
    );
    assert!(
        repo_path.join("ran.txt").exists(),
        "main profile command should have executed"
    );
}

#[test]
fn run_dispatch_streams_process_output_to_log_before_exit() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let (db_path, dir) = setup_db("run_dispatch_streams_process_output_to_log_before_exit");
    let data_dir = dir.path.join("data");
    std::env::set_var("FORGE_DATABASE_PATH", &db_path);
    std::env::set_var("FORGE_DATA_DIR", &data_dir);

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
            name: "stream-profile".to_string(),
            harness: "codex".to_string(),
            prompt_mode: "env".to_string(),
            command_template: "printf 'first\\n'; sleep 2; printf 'second\\n'".to_string(),
            ..Default::default()
        };
        profile_repo
            .create(&mut profile)
            .unwrap_or_else(|err| panic!("create profile: {err}"));

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "stream-loop".to_string(),
            repo_path: repo_path.to_string_lossy().into_owned(),
            profile_id: profile.id.clone(),
            base_prompt_msg: "hello".to_string(),
            max_iterations: 1,
            state: forge_db::loop_repository::LoopState::Stopped,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));
        loop_entry.id
    };

    let log_path = forge_cli::logs::default_log_path(
        data_dir.to_string_lossy().as_ref(),
        "stream-loop",
        &loop_id,
    );

    let run_handle = std::thread::spawn(|| run(&["run", "stream-loop"]));
    let deadline = Instant::now() + Duration::from_millis(1200);
    let mut saw_first_chunk = false;
    while Instant::now() < deadline {
        let log = std::fs::read_to_string(&log_path).unwrap_or_default();
        if log.contains("first") {
            saw_first_chunk = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(
        saw_first_chunk,
        "expected first output chunk in log before process exit; log: {}",
        std::fs::read_to_string(&log_path).unwrap_or_default()
    );
    assert!(
        !run_handle.is_finished(),
        "run completed before stream assertion; output was not observed in-flight"
    );

    let (code, stdout, stderr) = run_handle
        .join()
        .unwrap_or_else(|_| panic!("join run thread failed"));
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());

    let full_log = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|err| panic!("read log {}: {err}", log_path));
    assert!(full_log.contains("first"), "full log: {full_log}");
    assert!(full_log.contains("second"), "full log: {full_log}");
}

#[test]
fn run_dispatch_streams_output_for_all_harness_kinds() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let harnesses = ["codex", "claude", "opencode", "pi", "droid"];

    for harness in harnesses {
        let test_tag = format!(
            "run_dispatch_streams_all_harnesses_{}",
            sanitize_tag(harness)
        );
        let (db_path, dir) = setup_db(&test_tag);
        let data_dir = dir.path.join("data");
        std::env::set_var("FORGE_DATABASE_PATH", &db_path);
        std::env::set_var("FORGE_DATA_DIR", &data_dir);

        let loop_name = format!("stream-{harness}");
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
                name: format!("{harness}-stream-profile"),
                harness: harness.to_string(),
                prompt_mode: "env".to_string(),
                command_template: "printf 'first\\n'; sleep 1; printf 'second\\n'".to_string(),
                ..Default::default()
            };
            profile_repo
                .create(&mut profile)
                .unwrap_or_else(|err| panic!("create profile ({harness}): {err}"));

            let mut loop_entry = forge_db::loop_repository::Loop {
                name: loop_name.clone(),
                repo_path: repo_path.to_string_lossy().into_owned(),
                profile_id: profile.id.clone(),
                base_prompt_msg: "hello".to_string(),
                max_iterations: 1,
                state: forge_db::loop_repository::LoopState::Stopped,
                ..Default::default()
            };
            loop_repo
                .create(&mut loop_entry)
                .unwrap_or_else(|err| panic!("create loop ({harness}): {err}"));
            loop_entry.id
        };

        let log_path = forge_cli::logs::default_log_path(
            data_dir.to_string_lossy().as_ref(),
            &loop_name,
            &loop_id,
        );
        let loop_name_clone = loop_name.clone();
        let run_handle = std::thread::spawn(move || run(&["run", &loop_name_clone]));
        let deadline = Instant::now() + Duration::from_millis(700);
        let mut saw_first_chunk = false;
        while Instant::now() < deadline {
            let log = std::fs::read_to_string(&log_path).unwrap_or_default();
            if log.contains("first") {
                saw_first_chunk = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(40));
        }

        assert!(
            saw_first_chunk,
            "harness={harness} log={}",
            std::fs::read_to_string(&log_path).unwrap_or_default()
        );
        assert!(
            !run_handle.is_finished(),
            "harness={harness} run ended before in-flight log check"
        );

        let (code, stdout, stderr) = run_handle
            .join()
            .unwrap_or_else(|_| panic!("join run thread failed ({harness})"));
        assert_eq!(code, 0, "harness={harness} stderr: {stderr}");
        assert!(stdout.is_empty(), "harness={harness} stdout: {stdout}");
        assert!(stderr.is_empty(), "harness={harness} stderr: {stderr}");
    }
}

fn sanitize_tag(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
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
