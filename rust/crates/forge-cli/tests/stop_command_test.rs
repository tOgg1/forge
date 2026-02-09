use forge_cli::stop::{
    run_for_test, CommandOutput, InMemoryStopBackend, LoopRecord, LoopState, StopBackend,
};

#[test]
fn stop_single_json_matches_golden() {
    let mut backend = seeded();
    let out = run(&["stop", "oracle-loop", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/stop/single_json.json"));
}

#[test]
fn stop_multi_json_matches_golden() {
    let mut backend = seeded();
    let out = run(&["stop", "--all", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/stop/multi_json.json"));
}

#[test]
fn stop_single_text_matches_golden() {
    let mut backend = seeded();
    let out = run(&["stop", "oracle-loop"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/stop/single_text.txt"));
}

#[test]
fn stop_single_jsonl_matches_golden() {
    let mut backend = seeded();
    let out = run(&["stop", "oracle-loop", "--jsonl"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/stop/single_jsonl.json"));
}

#[test]
fn stop_no_selector_returns_error() {
    let mut backend = seeded();
    let out = run(&["stop"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "specify a loop or selector\n");
}

#[test]
fn stop_no_match_returns_error() {
    let mut backend = InMemoryStopBackend::default();
    let out = run(&["stop", "--all"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "no loops matched\n");
}

#[test]
fn stop_enqueues_for_matched_loops() {
    let mut backend = seeded();
    let out = run(&["stop", "--all", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(backend.enqueued, vec!["loop-001", "loop-002"]);
}

#[test]
fn stop_filters_by_pool() {
    let mut backend = seeded();
    let out = run(&["stop", "--pool", "burst", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(
        out.stdout,
        "{\n  \"action\": \"stop_graceful\",\n  \"loops\": 1\n}\n"
    );
    assert_eq!(backend.enqueued, vec!["loop-002"]);
}

#[test]
fn stop_integration_scenario() {
    let mut backend = seeded();

    let one = run(&["stop", "oracle-loop", "--json"], &mut backend);
    assert_success(&one);
    assert_eq!(
        one.stdout,
        "{\n  \"action\": \"stop_graceful\",\n  \"loops\": 1\n}\n"
    );
    assert_eq!(backend.enqueued, vec!["loop-001"]);

    let two = run(&["stop", "--pool", "burst", "--jsonl"], &mut backend);
    assert_success(&two);
    assert_eq!(two.stdout, "{\"action\":\"stop_graceful\",\"loops\":1}\n");
    assert_eq!(backend.enqueued, vec!["loop-001", "loop-002"]);
}

fn seeded() -> InMemoryStopBackend {
    InMemoryStopBackend::with_loops(vec![
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

fn run(args: &[&str], backend: &mut dyn StopBackend) -> CommandOutput {
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
