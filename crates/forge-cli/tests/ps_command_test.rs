use forge_cli::ps::{
    run_for_test, CommandOutput, InMemoryPsBackend, LoopRecord, LoopState, PsBackend,
};

#[test]
fn ps_single_json_matches_golden() {
    let backend = InMemoryPsBackend::with_loops(vec![sample_loop()]);
    let out = run(&["ps", "--json"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/ps/single_json.json"));
}

#[test]
fn ps_multi_json_matches_golden() {
    let backend = seeded();
    let out = run(&["ps", "--json"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/ps/multi_json.json"));
}

#[test]
fn ps_single_jsonl_matches_golden() {
    let backend = InMemoryPsBackend::with_loops(vec![sample_loop()]);
    let out = run(&["ps", "--jsonl"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/ps/single_jsonl.json"));
}

#[test]
fn ps_empty_text_matches_golden() {
    let backend = InMemoryPsBackend::default();
    let out = run(&["ps"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/ps/empty_text.txt"));
}

#[test]
fn ps_empty_json_matches_golden() {
    let backend = InMemoryPsBackend::default();
    let out = run(&["ps", "--json"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/ps/empty_json.json"));
}

#[test]
fn ps_single_text_matches_golden() {
    let backend = InMemoryPsBackend::with_loops(vec![sample_loop()]);
    let out = run(&["ps", "--no-color"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/ps/single_text.txt"));
}

#[test]
fn ps_no_selector_still_works() {
    let backend = seeded();
    let out = run(&["ps"], &backend);
    assert_success(&out);
    assert!(out.stdout.contains("oracle-loop"));
    assert!(out.stdout.contains("beta-loop"));
}

#[test]
fn ps_filters_by_pool() {
    let backend = seeded();
    let out = run(&["ps", "--pool", "burst", "--json"], &backend);
    assert_success(&out);
    let parsed = parse_json(&out.stdout);
    let arr = json_array(&parsed);
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "beta-loop");
}

#[test]
fn ps_filters_by_state() {
    let backend = seeded();
    let out = run(&["ps", "--state", "running", "--json"], &backend);
    assert_success(&out);
    let parsed = parse_json(&out.stdout);
    let arr = json_array(&parsed);
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "oracle-loop");
}

#[test]
fn ps_ls_alias_works() {
    let backend = seeded();
    let out = run(&["ls", "--json"], &backend);
    assert_success(&out);
    let parsed = parse_json(&out.stdout);
    let arr = json_array(&parsed);
    assert_eq!(arr.len(), 2);
}

#[test]
fn ps_quiet_suppresses_output() {
    let backend = seeded();
    let out = run(&["ps", "--quiet"], &backend);
    assert_success(&out);
    assert!(out.stdout.is_empty());
}

#[test]
fn ps_rejects_positional_args() {
    let backend = seeded();
    let out = run(&["ps", "some-loop"], &backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("no positional arguments"));
}

#[test]
fn ps_integration_scenario() {
    let backend = seeded();

    // List all as JSON
    let all = run(&["ps", "--json"], &backend);
    assert_success(&all);
    let parsed = parse_json(&all.stdout);
    assert_eq!(json_array(&parsed).len(), 2);

    // Filter by pool
    let filtered = run(&["ps", "--pool", "default", "--json"], &backend);
    assert_success(&filtered);
    let parsed = parse_json(&filtered.stdout);
    assert_eq!(json_array(&parsed).len(), 1);
    assert_eq!(parsed[0]["name"], "oracle-loop");

    // Empty filter returns empty
    let empty = run(&["ps", "--tag", "nonexistent", "--json"], &backend);
    assert_success(&empty);
    assert_eq!(empty.stdout, "[]\n");
}

fn parse_json(raw: &str) -> serde_json::Value {
    match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(err) => panic!("expected valid json: {err}\nraw:\n{raw}"),
    }
}

fn json_array(value: &serde_json::Value) -> &Vec<serde_json::Value> {
    match value.as_array() {
        Some(items) => items,
        None => panic!("expected json array, got: {value:?}"),
    }
}

fn seeded() -> InMemoryPsBackend {
    InMemoryPsBackend::with_loops(vec![
        LoopRecord {
            id: "loop-001".to_string(),
            short_id: "orc01".to_string(),
            name: "oracle-loop".to_string(),
            repo: "/repo/alpha".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec!["team-a".to_string()],
            runs: 5,
            pending_queue: 2,
            last_run: "2025-01-01T00:00:00Z".to_string(),
            wait_until: String::new(),
            runner_owner: "local".to_string(),
            runner_instance_id: "inst-001".to_string(),
            runner_pid_alive: Some(true),
            runner_daemon_alive: None,
        },
        LoopRecord {
            id: "loop-002".to_string(),
            short_id: "beta02".to_string(),
            name: "beta-loop".to_string(),
            repo: "/repo/beta".to_string(),
            pool: "burst".to_string(),
            profile: "claude".to_string(),
            state: LoopState::Stopped,
            tags: vec!["team-b".to_string()],
            runs: 0,
            pending_queue: 0,
            last_run: String::new(),
            wait_until: String::new(),
            runner_owner: String::new(),
            runner_instance_id: String::new(),
            runner_pid_alive: None,
            runner_daemon_alive: None,
        },
    ])
}

fn sample_loop() -> LoopRecord {
    LoopRecord {
        id: "loop-001".to_string(),
        short_id: "orc01".to_string(),
        name: "oracle-loop".to_string(),
        repo: "/repo/alpha".to_string(),
        pool: "default".to_string(),
        profile: "codex".to_string(),
        state: LoopState::Stopped,
        tags: vec!["team-a".to_string()],
        runs: 5,
        pending_queue: 2,
        last_run: "2025-01-01T00:00:00Z".to_string(),
        wait_until: String::new(),
        runner_owner: "local".to_string(),
        runner_instance_id: "inst-001".to_string(),
        runner_pid_alive: None,
        runner_daemon_alive: None,
    }
}

fn run(args: &[&str], backend: &dyn PsBackend) -> CommandOutput {
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
