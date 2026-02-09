use forge_cli::profile::{run_for_test, CommandOutput, InMemoryProfileBackend, ProfileBackend};

#[test]
fn profile_add_edit_list_remove_match_goldens() {
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
    assert_eq!(init.stdout, "No shell aliases found\n");
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
    run_for_test(args, backend)
}

fn assert_success(output: &CommandOutput) {
    assert_eq!(output.exit_code, 0);
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        output.stderr
    );
}
