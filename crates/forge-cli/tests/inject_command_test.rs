#![allow(clippy::unwrap_used, clippy::expect_used)]

use forge_cli::inject::{
    run_for_test, AgentRecord, AgentState, CommandOutput, InMemoryInjectBackend, InjectBackend,
};

// ---------------------------------------------------------------------------
// Golden tests – human output
// ---------------------------------------------------------------------------

#[test]
fn inject_idle_human_matches_golden() {
    let mut backend = single_idle_backend();
    let out = run(
        &["inject", "agent-inject-idle", "Stop and commit"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/inject/idle_human.txt"));
}

#[test]
fn inject_force_human_matches_golden() {
    let mut backend = multi_agent_backend();
    let out = run(
        &["inject", "--force", "agent-inject-busy", "Emergency stop"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/inject/force_human.txt"));
}

#[test]
fn inject_help_matches_golden() {
    let mut backend = single_idle_backend();
    let out = run(&["inject", "--help"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/inject/help.txt"));
}

// ---------------------------------------------------------------------------
// Golden tests – JSON output
// ---------------------------------------------------------------------------

#[test]
fn inject_idle_json_matches_golden() {
    let mut backend = single_idle_backend();
    let out = run(
        &["inject", "agent-inject-idle", "Stop and commit", "--json"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/inject/idle_json.json"));
}

#[test]
fn inject_idle_jsonl_matches_golden() {
    let mut backend = single_idle_backend();
    let out = run(
        &["inject", "agent-inject-idle", "hello", "--jsonl"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/inject/idle_jsonl.json"));
}

#[test]
fn inject_force_json_matches_golden() {
    let mut backend = multi_agent_backend();
    let out = run(
        &[
            "inject",
            "--force",
            "agent-inject-busy",
            "forced msg",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/inject/force_json.json"));
}

// ---------------------------------------------------------------------------
// JSON output structure tests
// ---------------------------------------------------------------------------

#[test]
fn inject_json_has_correct_structure() {
    let mut backend = single_idle_backend();
    let out = run(
        &["inject", "agent-inject-idle", "Stop and commit", "--json"],
        &mut backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["injected"], true);
    assert_eq!(parsed["agent_id"], "agent-inject-idle");
    assert_eq!(parsed["message"], "Stop and commit");
    assert_eq!(parsed["bypassed_queue"], true);
    assert_eq!(parsed["agent_state"], "idle");
}

#[test]
fn inject_force_json_has_correct_state() {
    let mut backend = multi_agent_backend();
    let out = run(
        &[
            "inject",
            "--force",
            "agent-inject-busy",
            "forced msg",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["injected"], true);
    assert_eq!(parsed["agent_id"], "agent-inject-busy");
    assert_eq!(parsed["agent_state"], "working");
    assert_eq!(parsed["bypassed_queue"], true);
}

#[test]
fn inject_jsonl_single_line() {
    let mut backend = single_idle_backend();
    let out = run(
        &["inject", "agent-inject-idle", "hello", "--jsonl"],
        &mut backend,
    );
    assert_success(&out);
    let lines: Vec<&str> = out.stdout.trim().lines().collect();
    assert_eq!(lines.len(), 1);
    let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(parsed["injected"], true);
    assert_eq!(parsed["bypassed_queue"], true);
}

// ---------------------------------------------------------------------------
// File/stdin message source tests
// ---------------------------------------------------------------------------

#[test]
fn inject_from_file_succeeds() {
    let mut backend =
        InMemoryInjectBackend::with_agents(vec![idle_agent()]).with_file("msg.txt", "file message");
    let out = run(
        &["inject", "agent-inject-idle", "--file", "msg.txt", "--json"],
        &mut backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["message"], "file message");
}

#[test]
fn inject_from_stdin_succeeds() {
    let mut backend =
        InMemoryInjectBackend::with_agents(vec![idle_agent()]).with_stdin("stdin message");
    let out = run(
        &["inject", "agent-inject-idle", "--stdin", "--json"],
        &mut backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["message"], "stdin message");
}

#[test]
fn inject_multiple_sources_errors() {
    let mut backend =
        InMemoryInjectBackend::with_agents(vec![idle_agent()]).with_file("msg.txt", "content");
    let out = run(
        &["inject", "agent-inject-idle", "inline", "--file", "msg.txt"],
        &mut backend,
    );
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("choose only one message source"));
}

// ---------------------------------------------------------------------------
// Integration scenario
// ---------------------------------------------------------------------------

#[test]
fn inject_integration_scenario() {
    // Step 1: inject to idle agent succeeds without --force.
    let mut backend = multi_agent_backend();
    let out = run(
        &["inject", "agent-inject-idle", "ping", "--json"],
        &mut backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["injected"], true);
    assert_eq!(parsed["agent_state"], "idle");
    assert_eq!(backend.sent_messages.len(), 1);

    // Step 2: inject to busy agent without --force fails.
    let out2 = run(&["inject", "agent-inject-busy", "hello"], &mut backend);
    assert_eq!(out2.exit_code, 1);
    assert!(out2.stderr.contains("agent is working"));
    assert_eq!(backend.sent_messages.len(), 1); // no new send

    // Step 3: inject to busy agent with --force succeeds.
    let out3 = run(
        &[
            "inject",
            "--force",
            "agent-inject-busy",
            "forced msg",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&out3);
    let parsed3: serde_json::Value = serde_json::from_str(&out3.stdout).unwrap();
    assert_eq!(parsed3["injected"], true);
    assert_eq!(parsed3["agent_state"], "working");
    assert_eq!(backend.sent_messages.len(), 2);
    assert_eq!(backend.sent_messages[1].0, "agent-inject-busy");
    assert_eq!(backend.sent_messages[1].1, "forced msg");
}

// ---------------------------------------------------------------------------
// Agent state matrix
// ---------------------------------------------------------------------------

#[test]
fn inject_state_matrix() {
    let states = [
        (AgentState::Idle, true),
        (AgentState::Stopped, true),
        (AgentState::Starting, true),
        (AgentState::Working, false),
        (AgentState::AwaitingApproval, false),
        (AgentState::Paused, false),
        (AgentState::RateLimited, false),
        (AgentState::Error, false),
    ];

    for (state, expect_success) in &states {
        let mut backend = InMemoryInjectBackend::with_agents(vec![AgentRecord {
            id: "agent-matrix".to_string(),
            workspace_id: "ws-001".to_string(),
            state: *state,
        }]);
        let out = run(&["inject", "agent-matrix", "hello", "--json"], &mut backend);
        assert_eq!(
            out.exit_code == 0,
            *expect_success,
            "state {:?}: expected success={}, got exit_code={}; stderr={}",
            state,
            expect_success,
            out.exit_code,
            out.stderr
        );
    }
}

#[test]
fn inject_force_bypasses_all_states() {
    let states = [
        AgentState::Working,
        AgentState::AwaitingApproval,
        AgentState::Paused,
        AgentState::RateLimited,
        AgentState::Error,
    ];

    for state in &states {
        let mut backend = InMemoryInjectBackend::with_agents(vec![AgentRecord {
            id: "agent-force".to_string(),
            workspace_id: "ws-001".to_string(),
            state: *state,
        }]);
        let out = run(
            &["inject", "--force", "agent-force", "hello", "--json"],
            &mut backend,
        );
        assert_success(&out);
    }
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn inject_no_agents_errors() {
    let mut backend = InMemoryInjectBackend::default();
    let out = run(&["inject"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("no agents in workspace"));
}

#[test]
fn inject_agent_not_found_errors() {
    let mut backend = single_idle_backend();
    let out = run(&["inject", "nonexistent", "hello"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("agent not found"));
}

#[test]
fn inject_empty_message_errors() {
    let mut backend = single_idle_backend();
    let out = run(&["inject", "agent-inject-idle"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("message required"));
}

#[test]
fn inject_send_failure_errors() {
    let mut backend =
        InMemoryInjectBackend::with_agents(vec![idle_agent()]).with_send_error("tmux error");
    let out = run(&["inject", "agent-inject-idle", "hello"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("failed to inject message"));
}

#[test]
fn inject_unknown_flag_errors() {
    let mut backend = single_idle_backend();
    let out = run(
        &["inject", "--unknown", "agent-inject-idle", "hello"],
        &mut backend,
    );
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("unknown flag"));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn idle_agent() -> AgentRecord {
    AgentRecord {
        id: "agent-inject-idle".to_string(),
        workspace_id: "ws-001".to_string(),
        state: AgentState::Idle,
    }
}

fn busy_agent() -> AgentRecord {
    AgentRecord {
        id: "agent-inject-busy".to_string(),
        workspace_id: "ws-001".to_string(),
        state: AgentState::Working,
    }
}

fn single_idle_backend() -> InMemoryInjectBackend {
    InMemoryInjectBackend::with_agents(vec![idle_agent()])
}

fn multi_agent_backend() -> InMemoryInjectBackend {
    InMemoryInjectBackend::with_agents(vec![idle_agent(), busy_agent()])
}

fn run(args: &[&str], backend: &mut dyn InjectBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(out: &CommandOutput) {
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
}
