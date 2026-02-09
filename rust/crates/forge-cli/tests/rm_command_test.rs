use forge_cli::rm::{
    run_for_test, CommandOutput, InMemoryLoopBackend, LoopBackend, LoopRecord, LoopState,
};

#[test]
fn rm_single_json_matches_golden() {
    let mut backend = seeded();
    let out = run(&["rm", "alpha", "--force", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/rm/single_json.json"));
}

#[test]
fn rm_multi_json_matches_golden() {
    let mut backend = seeded();
    let out = run(
        &["rm", "--repo", "/repo/alpha", "--force", "--json"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/rm/multi_json.json"));
}

#[test]
fn rm_force_guard_matches_go_behavior() {
    let mut backend = seeded();
    let out = run(&["rm", "--all"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "selector-based removal requires --force\n");
}

#[test]
fn rm_human_single_output_matches_golden() {
    let mut backend = seeded();
    let out = run(&["rm", "alpha", "--force"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/rm/single_text.txt"));
}

#[test]
fn rm_integration_scenario_removes_expected_loops() {
    let mut backend = seeded();

    let one = run(&["rm", "alpha", "--force", "--json"], &mut backend);
    assert_success(&one);
    assert_eq!(
        one.stdout,
        "{\n  \"removed\": 1,\n  \"loop_id\": \"loop-001\",\n  \"name\": \"alpha\"\n}\n"
    );

    let many = run(
        &["rm", "--repo", "/repo/alpha", "--force", "--jsonl"],
        &mut backend,
    );
    assert_success(&many);
    assert_eq!(
        many.stdout,
        "{\"removed\":1,\"loop_ids\":[\"loop-002\"],\"names\":[\"alpha-sibling\"]}\n"
    );

    let remaining = backend.list_loops().unwrap_or_default();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].name, "gamma-running");
}

fn seeded() -> InMemoryLoopBackend {
    InMemoryLoopBackend::with_loops(vec![
        LoopRecord {
            id: "loop-001".to_string(),
            short_id: "alpha01".to_string(),
            name: "alpha".to_string(),
            repo: "/repo/alpha".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Stopped,
            tags: vec!["team-a".to_string()],
        },
        LoopRecord {
            id: "loop-002".to_string(),
            short_id: "alpha02".to_string(),
            name: "alpha-sibling".to_string(),
            repo: "/repo/alpha".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Stopped,
            tags: vec!["team-a".to_string()],
        },
        LoopRecord {
            id: "loop-003".to_string(),
            short_id: "gamma03".to_string(),
            name: "gamma-running".to_string(),
            repo: "/repo/gamma".to_string(),
            pool: "burst".to_string(),
            profile: "claude".to_string(),
            state: LoopState::Running,
            tags: vec!["team-b".to_string()],
        },
    ])
}

fn run(args: &[&str], backend: &mut dyn LoopBackend) -> CommandOutput {
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
