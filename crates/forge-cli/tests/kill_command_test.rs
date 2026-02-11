use forge_cli::kill::{
    run_for_test, CommandOutput, InMemoryKillBackend, KillBackend, LoopRecord, LoopState,
};

#[test]
fn kill_single_json_matches_golden() {
    let mut backend = seeded();
    let out = run(&["kill", "oracle-loop", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/kill/single_json.json"));
}

#[test]
fn kill_multi_json_matches_golden() {
    let mut backend = seeded();
    let out = run(&["kill", "--all", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/kill/multi_json.json"));
}

#[test]
fn kill_single_text_matches_golden() {
    let mut backend = seeded();
    let out = run(&["kill", "oracle-loop"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/kill/single_text.txt"));
}

#[test]
fn kill_single_jsonl_matches_golden() {
    let mut backend = seeded();
    let out = run(&["kill", "oracle-loop", "--jsonl"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/kill/single_jsonl.json"));
}

#[test]
fn kill_no_selector_returns_error() {
    let mut backend = seeded();
    let out = run(&["kill"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "specify a loop or selector\n");
}

#[test]
fn kill_no_match_returns_error() {
    let mut backend = InMemoryKillBackend::default();
    let out = run(&["kill", "--all"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "no loops matched\n");
}

#[test]
fn kill_enqueues_for_matched_loops() {
    let mut backend = seeded();
    let out = run(&["kill", "--all", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(backend.enqueued, vec!["loop-001", "loop-002"]);
}

#[test]
fn kill_filters_by_pool() {
    let mut backend = seeded();
    let out = run(&["kill", "--pool", "burst", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(
        out.stdout,
        "{\n  \"action\": \"kill_now\",\n  \"loops\": 1\n}\n"
    );
    assert_eq!(backend.enqueued, vec!["loop-002"]);
}

#[test]
fn kill_integration_scenario() {
    let mut backend = seeded();

    let one = run(&["kill", "oracle-loop", "--json"], &mut backend);
    assert_success(&one);
    assert_eq!(
        one.stdout,
        "{\n  \"action\": \"kill_now\",\n  \"loops\": 1\n}\n"
    );
    assert_eq!(backend.enqueued, vec!["loop-001"]);

    let two = run(&["kill", "--pool", "burst", "--jsonl"], &mut backend);
    assert_success(&two);
    assert_eq!(two.stdout, "{\"action\":\"kill_now\",\"loops\":1}\n");
    assert_eq!(backend.enqueued, vec!["loop-001", "loop-002"]);
}

fn seeded() -> InMemoryKillBackend {
    InMemoryKillBackend::with_loops(vec![
        LoopRecord {
            id: "loop-001".to_string(),
            short_id: "orc01".to_string(),
            name: "oracle-loop".to_string(),
            repo: "/repo/alpha".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec!["team-a".to_string()],
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
        },
    ])
}

fn run(args: &[&str], backend: &mut dyn KillBackend) -> CommandOutput {
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
