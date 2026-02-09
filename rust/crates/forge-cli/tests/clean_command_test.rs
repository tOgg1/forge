use forge_cli::clean::{
    run_for_test, CommandOutput, InMemoryLoopBackend, LoopBackend, LoopRecord, LoopSelector,
    LoopState,
};

#[test]
fn clean_single_human_matches_golden() {
    let mut backend = InMemoryLoopBackend::with_loops(vec![LoopRecord {
        id: "loop-001".to_string(),
        name: "alpha".to_string(),
        repo: "/repo/alpha".to_string(),
        pool: "default".to_string(),
        profile: "codex".to_string(),
        tag: "team-a".to_string(),
        state: LoopState::Stopped,
    }]);
    let out = run(&["clean"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/clean/single_text.txt"));
}

#[test]
fn clean_single_json_matches_golden() {
    let mut backend = InMemoryLoopBackend::with_loops(vec![LoopRecord {
        id: "loop-001".to_string(),
        name: "alpha".to_string(),
        repo: "/repo/alpha".to_string(),
        pool: "default".to_string(),
        profile: "codex".to_string(),
        tag: "team-a".to_string(),
        state: LoopState::Stopped,
    }]);
    let out = run(&["clean", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/clean/single_json.json"));
}

#[test]
fn clean_many_json_with_skipped_matches_golden() {
    let mut backend = seeded();
    let out = run(&["clean", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/clean/multi_json.json"));
}

#[test]
fn clean_requires_inactive_match() {
    let mut backend = InMemoryLoopBackend::with_loops(vec![LoopRecord {
        id: "loop-003".to_string(),
        name: "gamma".to_string(),
        repo: "/repo/gamma".to_string(),
        pool: "burst".to_string(),
        profile: "claude".to_string(),
        tag: "team-b".to_string(),
        state: LoopState::Running,
    }]);
    let out = run(&["clean"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "no inactive loops matched\n");
}

#[test]
fn clean_integration_removes_only_inactive() {
    let mut backend = seeded();

    let first = run(&["clean", "--json"], &mut backend);
    assert_success(&first);
    assert_eq!(
        first.stdout,
        "{\n  \"removed\": 2,\n  \"loop_ids\": [\n    \"loop-001\",\n    \"loop-002\"\n  ],\n  \"names\": [\n    \"alpha\",\n    \"beta\"\n  ],\n  \"skipped\": 1\n}\n"
    );

    let remaining = match backend.select_loops(&LoopSelector::default()) {
        Ok(value) => value,
        Err(err) => panic!("backend select should succeed: {err}"),
    };
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].name, "gamma");
    assert_eq!(remaining[0].state, LoopState::Running);
}

fn seeded() -> InMemoryLoopBackend {
    InMemoryLoopBackend::with_loops(vec![
        LoopRecord {
            id: "loop-001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo/alpha".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            tag: "team-a".to_string(),
            state: LoopState::Stopped,
        },
        LoopRecord {
            id: "loop-002".to_string(),
            name: "beta".to_string(),
            repo: "/repo/alpha".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            tag: "team-a".to_string(),
            state: LoopState::Error,
        },
        LoopRecord {
            id: "loop-003".to_string(),
            name: "gamma".to_string(),
            repo: "/repo/gamma".to_string(),
            pool: "burst".to_string(),
            profile: "claude".to_string(),
            tag: "team-b".to_string(),
            state: LoopState::Running,
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
