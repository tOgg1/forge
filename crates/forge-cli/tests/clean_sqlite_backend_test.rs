use forge_cli::clean::{run_for_test, LoopBackend, LoopSelector, LoopState, SqliteCleanBackend};
use std::path::PathBuf;

#[test]
fn clean_sqlite_backend_removes_only_inactive_loops() {
    let (db_path, _dir) = setup_db("clean_sqlite_backend_removes_only_inactive_loops");

    {
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|e| panic!("open db {}: {e}", db_path.display()));
        db.migrate_up().unwrap_or_else(|e| panic!("migrate: {e}"));

        let pool_repo = forge_db::pool_repository::PoolRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);

        let mut pool = forge_db::pool_repository::Pool {
            name: "default".to_string(),
            is_default: true,
            ..Default::default()
        };
        pool_repo
            .create(&mut pool)
            .unwrap_or_else(|e| panic!("pool create: {e}"));

        let mut profile = forge_db::profile_repository::Profile {
            name: "codex".to_string(),
            command_template: "codex".to_string(),
            ..Default::default()
        };
        profile_repo
            .create(&mut profile)
            .unwrap_or_else(|e| panic!("profile create: {e}"));

        let repo_path = std::env::current_dir()
            .unwrap_or_else(|e| panic!("cwd: {e}"))
            .to_string_lossy()
            .into_owned();

        let mut stopped = forge_db::loop_repository::Loop {
            name: "alpha".to_string(),
            repo_path: repo_path.clone(),
            pool_id: pool.id.clone(),
            profile_id: profile.id.clone(),
            state: forge_db::loop_repository::LoopState::Stopped,
            tags: vec!["team-a".to_string()],
            ..Default::default()
        };
        loop_repo
            .create(&mut stopped)
            .unwrap_or_else(|e| panic!("loop create stopped: {e}"));

        let mut errored = forge_db::loop_repository::Loop {
            name: "beta".to_string(),
            repo_path: repo_path.clone(),
            pool_id: pool.id.clone(),
            profile_id: profile.id.clone(),
            state: forge_db::loop_repository::LoopState::Error,
            tags: vec!["team-a".to_string()],
            ..Default::default()
        };
        loop_repo
            .create(&mut errored)
            .unwrap_or_else(|e| panic!("loop create error: {e}"));

        let mut running = forge_db::loop_repository::Loop {
            name: "gamma".to_string(),
            repo_path,
            pool_id: pool.id.clone(),
            profile_id: profile.id.clone(),
            state: forge_db::loop_repository::LoopState::Running,
            tags: vec!["team-b".to_string()],
            ..Default::default()
        };
        loop_repo
            .create(&mut running)
            .unwrap_or_else(|e| panic!("loop create running: {e}"));
    }

    let mut backend = SqliteCleanBackend::new(db_path.clone());

    let before = backend
        .select_loops(&LoopSelector::default())
        .unwrap_or_else(|e| panic!("select before: {e}"));
    assert_eq!(before.len(), 3);

    let out = run_for_test(&["clean", "--json"], &mut backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());

    let remaining = backend
        .select_loops(&LoopSelector::default())
        .unwrap_or_else(|e| panic!("select after: {e}"));
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].name, "gamma");
    assert_eq!(remaining[0].state, LoopState::Running);
}

#[test]
fn clean_sqlite_backend_tag_filter_applies_before_inactive_check() {
    let (db_path, _dir) = setup_db("clean_sqlite_backend_tag_filter_applies_before_inactive_check");

    {
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|e| panic!("open db {}: {e}", db_path.display()));
        db.migrate_up().unwrap_or_else(|e| panic!("migrate: {e}"));

        let pool_repo = forge_db::pool_repository::PoolRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);

        let mut pool = forge_db::pool_repository::Pool {
            name: "default".to_string(),
            is_default: true,
            ..Default::default()
        };
        pool_repo
            .create(&mut pool)
            .unwrap_or_else(|e| panic!("pool create: {e}"));

        let mut profile = forge_db::profile_repository::Profile {
            name: "codex".to_string(),
            command_template: "codex".to_string(),
            ..Default::default()
        };
        profile_repo
            .create(&mut profile)
            .unwrap_or_else(|e| panic!("profile create: {e}"));

        let repo_path = std::env::current_dir()
            .unwrap_or_else(|e| panic!("cwd: {e}"))
            .to_string_lossy()
            .into_owned();

        let mut stopped_team_a = forge_db::loop_repository::Loop {
            name: "alpha".to_string(),
            repo_path: repo_path.clone(),
            pool_id: pool.id.clone(),
            profile_id: profile.id.clone(),
            state: forge_db::loop_repository::LoopState::Stopped,
            tags: vec!["team-a".to_string()],
            ..Default::default()
        };
        loop_repo
            .create(&mut stopped_team_a)
            .unwrap_or_else(|e| panic!("loop create: {e}"));

        let mut stopped_team_b = forge_db::loop_repository::Loop {
            name: "beta".to_string(),
            repo_path,
            pool_id: pool.id.clone(),
            profile_id: profile.id.clone(),
            state: forge_db::loop_repository::LoopState::Stopped,
            tags: vec!["team-b".to_string()],
            ..Default::default()
        };
        loop_repo
            .create(&mut stopped_team_b)
            .unwrap_or_else(|e| panic!("loop create: {e}"));
    }

    let mut backend = SqliteCleanBackend::new(db_path.clone());
    let out = run_for_test(&["clean", "--tag", "team-a", "--json"], &mut backend);
    assert_eq!(out.exit_code, 0);

    let remaining = backend
        .select_loops(&LoopSelector::default())
        .unwrap_or_else(|e| panic!("select after: {e}"));
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].name, "beta");
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
        std::fs::create_dir_all(&path).unwrap_or_else(|e| panic!("mkdir {}: {e}", path.display()));
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best-effort; tests shouldn't fail on cleanup.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
