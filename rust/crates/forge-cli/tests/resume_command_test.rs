use forge_cli::resume::{
    run_for_test, CommandOutput, InMemoryResumeBackend, LoopRecord, LoopState, ResumeBackend,
};

#[test]
fn resume_human_and_json_outputs_match_goldens() {
    let mut backend = InMemoryResumeBackend::with_loops(vec![LoopRecord {
        id: "loop-1".to_string(),
        short_id: "abc123".to_string(),
        name: "demo".to_string(),
        state: LoopState::Stopped,
        runner_owner: String::new(),
        runner_instance_id: String::new(),
    }]);

    let human = run(&["resume", "demo"], &mut backend);
    assert_success(&human);
    assert_eq!(human.stdout, include_str!("golden/resume/resume_human.txt"));

    let mut backend = InMemoryResumeBackend::with_loops(vec![LoopRecord {
        id: "loop-2".to_string(),
        short_id: "def456".to_string(),
        name: "nightly".to_string(),
        state: LoopState::Error,
        runner_owner: String::new(),
        runner_instance_id: String::new(),
    }]);

    let json = run(
        &["resume", "nightly", "--spawn-owner", "daemon", "--json"],
        &mut backend,
    );
    assert_success(&json);
    assert_eq!(json.stdout, include_str!("golden/resume/resume_json.txt"));
}

#[test]
fn resume_integration_scenario_updates_state() {
    let mut backend = InMemoryResumeBackend::with_loops(vec![LoopRecord {
        id: "loop-1".to_string(),
        short_id: "abc123".to_string(),
        name: "demo".to_string(),
        state: LoopState::Stopped,
        runner_owner: String::new(),
        runner_instance_id: String::new(),
    }]);

    let quiet = run(
        &["resume", "abc123", "--spawn-owner", "local", "--quiet"],
        &mut backend,
    );
    assert_success(&quiet);
    assert!(quiet.stdout.is_empty());

    let loops = match backend.list_loops() {
        Ok(value) => value,
        Err(err) => panic!("list_loops should work: {err}"),
    };
    assert_eq!(loops.len(), 1);
    assert_eq!(loops[0].state, LoopState::Running);
    assert_eq!(loops[0].runner_owner, "local");
    assert_eq!(loops[0].runner_instance_id, "resume-001");
}

#[test]
fn resume_error_paths() {
    let mut backend = InMemoryResumeBackend::with_loops(vec![LoopRecord {
        id: "loop-1".to_string(),
        short_id: "abc123".to_string(),
        name: "demo".to_string(),
        state: LoopState::Running,
        runner_owner: "local".to_string(),
        runner_instance_id: "inst-1".to_string(),
    }]);

    let state_error = run(&["resume", "demo"], &mut backend);
    assert_eq!(state_error.exit_code, 1);
    assert!(state_error.stdout.is_empty());
    assert_eq!(
        state_error.stderr,
        "loop \"demo\" is running; only stopped or errored loops can be resumed\n"
    );

    let invalid_owner = run(
        &["resume", "demo", "--spawn-owner", "invalid"],
        &mut backend,
    );
    assert_eq!(invalid_owner.exit_code, 1);
    assert!(invalid_owner.stdout.is_empty());
    assert_eq!(
        invalid_owner.stderr,
        "invalid --spawn-owner value: invalid\n"
    );

    let missing_loop = run(&["resume"], &mut backend);
    assert_eq!(missing_loop.exit_code, 1);
    assert!(missing_loop.stdout.is_empty());
    assert_eq!(missing_loop.stderr, "loop name or ID required\n");
}

fn run(args: &[&str], backend: &mut dyn ResumeBackend) -> CommandOutput {
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
