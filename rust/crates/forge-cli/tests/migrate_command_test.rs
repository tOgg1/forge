use forge_cli::migrate::{
    run_for_test, CommandOutput, InMemoryMigrationBackend, MigrationBackend, MigrationStatus,
};

#[derive(Debug, Clone)]
struct ScriptedBackend {
    up_result: Result<usize, String>,
    down_result: Result<usize, String>,
    to_result: Result<(), String>,
    status_result: Result<Vec<MigrationStatus>, String>,
    version_result: Result<i32, String>,
    last_to: Option<i32>,
    last_down_steps: Option<i32>,
}

impl ScriptedBackend {
    fn success() -> Self {
        Self {
            up_result: Ok(0),
            down_result: Ok(0),
            to_result: Ok(()),
            status_result: Ok(Vec::new()),
            version_result: Ok(0),
            last_to: None,
            last_down_steps: None,
        }
    }
}

impl MigrationBackend for ScriptedBackend {
    fn migrate_up(&mut self) -> Result<usize, String> {
        self.up_result.clone()
    }

    fn migrate_to(&mut self, target_version: i32) -> Result<(), String> {
        self.last_to = Some(target_version);
        self.to_result.clone()
    }

    fn migrate_down(&mut self, steps: i32) -> Result<usize, String> {
        self.last_down_steps = Some(steps);
        self.down_result.clone()
    }

    fn migration_status(&mut self) -> Result<Vec<MigrationStatus>, String> {
        self.status_result.clone()
    }

    fn schema_version(&mut self) -> Result<i32, String> {
        self.version_result.clone()
    }
}

#[test]
fn migrate_up_applied_matches_golden() {
    let mut backend = ScriptedBackend::success();
    backend.up_result = Ok(3);
    let out = run(&["migrate", "up"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stderr, include_str!("golden/migrate/up_applied.txt"));
}

#[test]
fn migrate_up_no_pending_matches_golden() {
    let mut backend = ScriptedBackend::success();
    backend.up_result = Ok(0);
    let out = run(&["migrate", "up"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stderr, include_str!("golden/migrate/up_no_pending.txt"));
}

#[test]
fn migrate_up_to_version_matches_golden() {
    let mut backend = ScriptedBackend::success();
    let out = run(&["migrate", "up", "--to", "7"], &mut backend);
    assert_success(&out);
    assert_eq!(backend.last_to, Some(7));
    assert_eq!(out.stderr, include_str!("golden/migrate/up_to_version.txt"));
}

#[test]
fn migrate_down_default_steps_matches_golden() {
    let mut backend = ScriptedBackend::success();
    backend.down_result = Ok(1);
    let out = run(&["migrate", "down"], &mut backend);
    assert_success(&out);
    assert_eq!(backend.last_down_steps, Some(1));
    assert_eq!(
        out.stderr,
        include_str!("golden/migrate/down_rolled_back.txt")
    );
}

#[test]
fn migrate_down_none_matches_golden() {
    let mut backend = ScriptedBackend::success();
    backend.down_result = Ok(0);
    let out = run(&["migrate", "down", "--steps", "4"], &mut backend);
    assert_success(&out);
    assert_eq!(backend.last_down_steps, Some(4));
    assert_eq!(out.stderr, include_str!("golden/migrate/down_none.txt"));
}

#[test]
fn migrate_status_table_matches_golden() {
    let mut backend = ScriptedBackend::success();
    backend.status_result = Ok(vec![
        MigrationStatus {
            version: 1,
            description: "initial schema".to_string(),
            applied: true,
            applied_at: "tick-001".to_string(),
        },
        MigrationStatus {
            version: 2,
            description: "node connection prefs".to_string(),
            applied: false,
            applied_at: String::new(),
        },
    ]);

    let out = run(&["migrate", "status"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/migrate/status_table.txt"));
}

#[test]
fn migrate_status_json_matches_golden() {
    let mut backend = ScriptedBackend::success();
    backend.status_result = Ok(vec![
        MigrationStatus {
            version: 1,
            description: "initial schema".to_string(),
            applied: true,
            applied_at: "tick-001".to_string(),
        },
        MigrationStatus {
            version: 2,
            description: "node connection prefs".to_string(),
            applied: false,
            applied_at: String::new(),
        },
    ]);

    let out = run(&["migrate", "status", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/migrate/status_json.json"));
}

#[test]
fn migrate_version_outputs_match_goldens() {
    let mut backend = ScriptedBackend::success();
    backend.version_result = Ok(12);

    let text_out = run(&["migrate", "version"], &mut backend);
    assert_success(&text_out);
    assert_eq!(
        text_out.stderr,
        include_str!("golden/migrate/version_text.txt")
    );

    let json_out = run(&["migrate", "version", "--json"], &mut backend);
    assert_success(&json_out);
    assert_eq!(
        json_out.stdout,
        include_str!("golden/migrate/version_json.json")
    );
}

#[test]
fn migrate_integration_scenario_runs_end_to_end() {
    let mut backend = InMemoryMigrationBackend::default();

    let up_to = run(&["migrate", "up", "--to", "5"], &mut backend);
    assert_success(&up_to);
    assert_eq!(up_to.stderr, "Migrated to version 5\n");

    let down = run(&["migrate", "down", "-n", "2"], &mut backend);
    assert_success(&down);
    assert_eq!(down.stderr, "Rolled back 2 migration(s)\n");

    let version = run(&["migrate", "version", "--json"], &mut backend);
    assert_success(&version);
    assert_eq!(version.stdout, "{\"version\":3}\n");
}

#[test]
fn migrate_error_path_preserves_prefix_and_exit_code() {
    let mut backend = ScriptedBackend::success();
    backend.up_result = Err("boom".to_string());
    let out = run(&["migrate", "up"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "migration failed: boom\n");
}

#[test]
fn migrate_invalid_flag_value_returns_error() {
    let mut backend = ScriptedBackend::success();
    let out = run(&["migrate", "down", "--steps", "abc"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "error: invalid value 'abc' for --steps\n");
}

fn run(args: &[&str], backend: &mut dyn MigrationBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(output: &CommandOutput) {
    assert_eq!(output.exit_code, 0);
}
