#![allow(clippy::unwrap_used)]

use forge_cli::wait::{
    run_for_test, AgentRecord, AgentState, CommandOutput, InMemoryWaitBackend, WaitBackend,
};

#[test]
fn wait_idle_json_matches_golden() {
    let backend = seeded_idle();
    let out = run(
        &["wait", "--until", "idle", "--agent", "agent-001", "--json"],
        &backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/wait/idle_met_json.json"));
}

#[test]
fn wait_idle_jsonl_matches_golden() {
    let backend = seeded_idle();
    let out = run(
        &["wait", "--until", "idle", "--agent", "agent-001", "--jsonl"],
        &backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/wait/idle_met_jsonl.json"));
}

#[test]
fn wait_idle_text_matches_golden() {
    let backend = seeded_idle();
    let out = run(
        &["wait", "--until", "idle", "--agent", "agent-001"],
        &backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/wait/idle_met_text.txt"));
}

#[test]
fn wait_idle_timeout_json_matches_golden() {
    let mut backend = seeded_working();
    backend.deadline_exceeded = true;
    backend.elapsed_str = "30m0s".to_string();
    let out = run(
        &["wait", "--until", "idle", "--agent", "agent-002", "--json"],
        &backend,
    );
    assert_success(&out); // JSON mode returns exit 0 even on timeout
    assert_eq!(
        out.stdout,
        include_str!("golden/wait/idle_timeout_json.json")
    );
}

#[test]
fn wait_ready_json_matches_golden() {
    let backend = seeded_idle();
    let out = run(
        &["wait", "--until", "ready", "--agent", "agent-001", "--json"],
        &backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/wait/ready_met_json.json"));
}

#[test]
fn wait_all_idle_json_matches_golden() {
    let backend = seeded_idle();
    let out = run(
        &[
            "wait",
            "--until",
            "all-idle",
            "--workspace",
            "ws-001",
            "--json",
        ],
        &backend,
    );
    assert_success(&out);
    assert_eq!(
        out.stdout,
        include_str!("golden/wait/all_idle_met_json.json")
    );
}

#[test]
fn wait_invalid_condition_error() {
    let backend = InMemoryWaitBackend::default();
    let out = run(
        &[
            "wait",
            "--until",
            "definitely-invalid",
            "--agent",
            "agent-001",
        ],
        &backend,
    );
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        out.stderr,
        "invalid condition 'definitely-invalid'; valid conditions: [idle queue-empty cooldown-over all-idle any-idle ready]\n"
    );
}

#[test]
fn wait_missing_until_returns_error() {
    let backend = InMemoryWaitBackend::default();
    let out = run(&["wait"], &backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("--until is required"));
}

#[test]
fn wait_agent_required_no_context() {
    let backend = InMemoryWaitBackend::default();
    let out = run(&["wait", "--until", "idle"], &backend);
    assert_eq!(out.exit_code, 1);
    assert!(out
        .stderr
        .contains("--agent is required for condition 'idle' (no context set)"));
}

#[test]
fn wait_workspace_required_no_context() {
    let backend = InMemoryWaitBackend::default();
    let out = run(&["wait", "--until", "all-idle"], &backend);
    assert_eq!(out.exit_code, 1);
    assert!(out
        .stderr
        .contains("--workspace is required for condition 'all-idle' (no context set)"));
}

#[test]
fn wait_quiet_suppresses_output() {
    let backend = seeded_idle();
    let out = run(
        &["wait", "--until", "idle", "--agent", "agent-001", "--quiet"],
        &backend,
    );
    assert_success(&out);
    assert!(out.stdout.is_empty());
}

#[test]
fn wait_integration_scenario() {
    // Test idle condition met
    let backend = seeded_idle();
    let one = run(
        &["wait", "--until", "idle", "--agent", "agent-001", "--json"],
        &backend,
    );
    assert_success(&one);
    let parsed: serde_json::Value = serde_json::from_str(&one.stdout).unwrap();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["condition"], "idle");

    // Test ready condition met on same agent
    let two = run(
        &["wait", "--until", "ready", "--agent", "agent-001", "--json"],
        &backend,
    );
    assert_success(&two);
    let parsed2: serde_json::Value = serde_json::from_str(&two.stdout).unwrap();
    assert_eq!(parsed2["success"], true);
    assert_eq!(parsed2["condition"], "ready");

    // Test all-idle on workspace
    let three = run(
        &[
            "wait",
            "--until",
            "all-idle",
            "--workspace",
            "ws-001",
            "--jsonl",
        ],
        &backend,
    );
    assert_success(&three);
    let parsed3: serde_json::Value = serde_json::from_str(&three.stdout).unwrap();
    assert_eq!(parsed3["success"], true);
}

fn seeded_idle() -> InMemoryWaitBackend {
    InMemoryWaitBackend::with_agents(vec![AgentRecord {
        id: "agent-001".to_string(),
        workspace_id: "ws-001".to_string(),
        state: AgentState::Idle,
        account_id: "acc-001".to_string(),
        pending_queue_items: 0,
        cooldown_remaining_secs: None,
    }])
}

fn seeded_working() -> InMemoryWaitBackend {
    InMemoryWaitBackend::with_agents(vec![AgentRecord {
        id: "agent-002".to_string(),
        workspace_id: "ws-001".to_string(),
        state: AgentState::Working,
        account_id: "acc-001".to_string(),
        pending_queue_items: 0,
        cooldown_remaining_secs: None,
    }])
}

fn run(args: &[&str], backend: &dyn WaitBackend) -> CommandOutput {
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
