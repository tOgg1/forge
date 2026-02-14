use forge_cli::profile::{run_for_test, CommandOutput, InMemoryProfileBackend, ProfileBackend};
use std::ffi::OsString;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn profile_json_outputs_match_goldens() {
    let mut backend = InMemoryProfileBackend::default();

    let add = run(
        &[
            "profile",
            "add",
            "codex",
            "--name",
            "alpha",
            "--auth-kind",
            "codex",
            "--home",
            "/tmp/auth-alpha",
            "--prompt-mode",
            "env",
            "--command",
            "codex exec",
            "--model",
            "gpt-5",
            "--extra-arg",
            "--sandbox",
            "--env",
            "A=1",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&add);
    assert_eq!(add.stdout, include_str!("golden/profile/add_json.txt"));

    let edit = run(
        &["profile", "edit", "alpha", "--model", "gpt-5.1", "--json"],
        &mut backend,
    );
    assert_success(&edit);
    assert_eq!(edit.stdout, include_str!("golden/profile/edit_json.txt"));

    let list = run(&["profile", "ls", "--json"], &mut backend);
    assert_success(&list);
    assert_eq!(list.stdout, include_str!("golden/profile/list_json.txt"));

    let cooldown_set = run(
        &[
            "profile",
            "cooldown",
            "set",
            "alpha",
            "--until",
            "2026-02-10T00:00:00Z",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&cooldown_set);
    assert_eq!(
        cooldown_set.stdout,
        include_str!("golden/profile/cooldown_set_json.txt")
    );

    let cooldown_clear = run(
        &["profile", "cooldown", "clear", "alpha", "--json"],
        &mut backend,
    );
    assert_success(&cooldown_clear);
    assert_eq!(
        cooldown_clear.stdout,
        include_str!("golden/profile/cooldown_clear_json.txt")
    );

    // Deterministic doctor report: avoid auth_home/command checks which depend on local FS/PATH.
    let add_gamma = run(
        &[
            "profile",
            "add",
            "codex",
            "--name",
            "gamma",
            "--command",
            "",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&add_gamma);

    let doctor = run(&["profile", "doctor", "gamma", "--json"], &mut backend);
    assert_success(&doctor);
    assert_eq!(
        doctor.stdout,
        include_str!("golden/profile/doctor_json.txt")
    );

    let init = run(&["profile", "init", "--json"], &mut backend);
    assert_success(&init);
    assert_eq!(init.stdout, include_str!("golden/profile/init_json.txt"));

    let remove = run(&["profile", "rm", "alpha", "--json"], &mut backend);
    assert_success(&remove);
    assert_eq!(
        remove.stdout,
        include_str!("golden/profile/remove_json.txt")
    );
}

#[test]
fn profile_integration_scenario_human_paths() {
    let mut backend = InMemoryProfileBackend::default();

    let add = run(
        &["profile", "add", "claude", "--name", "beta"],
        &mut backend,
    );
    assert_success(&add);
    assert_eq!(add.stdout, "Profile \"beta\" created\n");

    let cooldown_set = run(
        &[
            "profile",
            "cooldown",
            "set",
            "beta",
            "--until",
            "2026-02-10T00:00:00Z",
        ],
        &mut backend,
    );
    assert_success(&cooldown_set);
    assert_eq!(
        cooldown_set.stdout,
        "Profile \"beta\" cooldown set to 2026-02-10T00:00:00Z\n"
    );

    let cooldown_clear = run(&["profile", "cooldown", "clear", "beta"], &mut backend);
    assert_success(&cooldown_clear);
    assert_eq!(cooldown_clear.stdout, "Profile \"beta\" cooldown cleared\n");

    let doctor = run(&["profile", "doctor", "beta"], &mut backend);
    assert_success(&doctor);
    assert!(doctor.stdout.contains("Profile beta"));

    let list = run(&["profile", "ls"], &mut backend);
    assert_success(&list);
    assert!(list.stdout.contains("NAME"));
    assert!(list.stdout.contains("beta"));

    let init = run(&["profile", "init"], &mut backend);
    assert_success(&init);
    assert_eq!(
        init.stdout,
        "No profiles imported from shell aliases/harnesses\n"
    );
}

#[test]
fn profile_validation_and_error_paths() {
    let mut backend = InMemoryProfileBackend::default();

    let missing_name = run(&["profile", "add", "codex"], &mut backend);
    assert_eq!(missing_name.exit_code, 1);
    assert!(missing_name.stdout.is_empty());
    assert_eq!(missing_name.stderr, "--name is required\n");

    let bad_env = run(
        &[
            "profile", "add", "codex", "--name", "alpha", "--env", "BROKEN",
        ],
        &mut backend,
    );
    assert_eq!(bad_env.exit_code, 1);
    assert!(bad_env.stdout.is_empty());
    assert_eq!(
        bad_env.stderr,
        "invalid env pair \"BROKEN\" (expected KEY=VALUE)\n"
    );

    let bad_harness = run(
        &["profile", "add", "unknown", "--name", "alpha"],
        &mut backend,
    );
    assert_eq!(bad_harness.exit_code, 1);
    assert!(bad_harness.stdout.is_empty());
    assert_eq!(bad_harness.stderr, "unknown harness \"unknown\"\n");

    let cooldown_missing_until = run(&["profile", "cooldown", "set", "alpha"], &mut backend);
    assert_eq!(cooldown_missing_until.exit_code, 1);
    assert!(cooldown_missing_until.stdout.is_empty());
    assert_eq!(cooldown_missing_until.stderr, "--until is required\n");
}

fn run(args: &[&str], backend: &mut dyn ProfileBackend) -> CommandOutput {
    with_profile_init_aliases_disabled(|| run_for_test(args, backend))
}

fn with_profile_init_aliases_disabled<T>(callback: impl FnOnce() -> T) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = match LOCK.get_or_init(|| Mutex::new(())).lock() {
        Ok(guard) => guard,
        Err(_) => panic!("profile env lock poisoned"),
    };

    let skip_key = "FORGE_PROFILE_INIT_SKIP_ZSH_ALIAS";
    let alias_file_key = "FORGE_PROFILE_INIT_ALIAS_FILE";
    let path_key = "PATH";
    let previous_skip: Option<OsString> = std::env::var_os(skip_key);
    let previous_alias_file: Option<OsString> = std::env::var_os(alias_file_key);
    let previous_path: Option<OsString> = std::env::var_os(path_key);

    std::env::set_var(skip_key, "1");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let isolated_alias_file = std::env::temp_dir().join(format!(
        "forge-profile-aliases-empty-{}-{nonce}",
        std::process::id()
    ));
    std::env::set_var(alias_file_key, &isolated_alias_file);
    std::env::set_var(path_key, "");

    let result = callback();

    match previous_skip {
        Some(value) => std::env::set_var(skip_key, value),
        None => std::env::remove_var(skip_key),
    }
    match previous_alias_file {
        Some(value) => std::env::set_var(alias_file_key, value),
        None => std::env::remove_var(alias_file_key),
    }
    match previous_path {
        Some(value) => std::env::set_var(path_key, value),
        None => std::env::remove_var(path_key),
    }
    result
}

fn assert_success(output: &CommandOutput) {
    assert_eq!(output.exit_code, 0);
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        output.stderr
    );
}
