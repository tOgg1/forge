use forge_cli::pool::{run_for_test, CommandOutput, InMemoryPoolBackend, PoolBackend};

#[test]
fn pool_json_outputs_match_goldens() {
    let mut backend = InMemoryPoolBackend::default();

    let create = run(&["pool", "create", "alpha", "--json"], &mut backend);
    assert_success(&create);
    assert_eq!(create.stdout, include_str!("golden/pool/create_json.json"));

    let add = run(
        &["pool", "add", "alpha", "profile-a", "profile-b", "--json"],
        &mut backend,
    );
    assert_success(&add);
    assert_eq!(add.stdout, include_str!("golden/pool/add_json.json"));

    let list = run(&["pool", "ls", "--json"], &mut backend);
    assert_success(&list);
    assert_eq!(list.stdout, include_str!("golden/pool/list_json.json"));

    let show = run(&["pool", "show", "alpha", "--json"], &mut backend);
    assert_success(&show);
    assert_eq!(show.stdout, include_str!("golden/pool/show_json.json"));
}

#[test]
fn pool_integration_scenario_human_paths() {
    let mut backend = InMemoryPoolBackend::default();

    let create_alpha = run(&["pool", "create", "alpha"], &mut backend);
    assert_success(&create_alpha);
    assert_eq!(create_alpha.stdout, "Pool \"alpha\" created\n");

    let create_beta = run(&["pool", "create", "beta"], &mut backend);
    assert_success(&create_beta);
    assert_eq!(create_beta.stdout, "Pool \"beta\" created\n");

    let set_default = run(&["pool", "set-default", "beta"], &mut backend);
    assert_success(&set_default);
    assert_eq!(set_default.stdout, "Default pool set to \"beta\"\n");

    let list = run(&["pool", "list"], &mut backend);
    assert_success(&list);
    assert!(list.stdout.contains("NAME"));
    assert!(list.stdout.contains("alpha"));
    assert!(list.stdout.contains("beta"));
    assert!(list.stdout.contains("yes"));

    let add = run(&["pool", "add", "beta", "profile-c"], &mut backend);
    assert_success(&add);
    assert_eq!(add.stdout, "Added profile-c to pool \"beta\"\n");

    let show = run(&["pool", "show", "beta"], &mut backend);
    assert_success(&show);
    assert!(show.stdout.contains("Pool beta"));
    assert!(show.stdout.contains("Default: yes"));
    assert!(show.stdout.contains("PROFILE"));
    assert!(show.stdout.contains("profile-c"));
}

#[test]
fn pool_validation_and_error_paths() {
    let mut backend = InMemoryPoolBackend::default();

    let missing_name = run(&["pool", "create"], &mut backend);
    assert_eq!(missing_name.exit_code, 1);
    assert!(missing_name.stdout.is_empty());
    assert_eq!(missing_name.stderr, "pool create requires <name>\n");

    let unknown_flag = run(&["pool", "create", "alpha", "--bogus"], &mut backend);
    assert_eq!(unknown_flag.exit_code, 1);
    assert!(unknown_flag.stdout.is_empty());
    assert_eq!(unknown_flag.stderr, "unknown pool create flag: --bogus\n");

    let missing_add = run(&["pool", "add", "alpha"], &mut backend);
    assert_eq!(missing_add.exit_code, 1);
    assert!(missing_add.stdout.is_empty());
    assert_eq!(
        missing_add.stderr,
        "pool add requires <pool> <profile...>\n"
    );

    let set_default_unknown = run(&["pool", "set-default", "missing"], &mut backend);
    assert_eq!(set_default_unknown.exit_code, 1);
    assert!(set_default_unknown.stdout.is_empty());
    assert_eq!(set_default_unknown.stderr, "pool not found: missing\n");
}

fn run(args: &[&str], backend: &mut dyn PoolBackend) -> CommandOutput {
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
