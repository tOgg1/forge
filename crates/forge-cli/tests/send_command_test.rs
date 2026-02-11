#![allow(clippy::unwrap_used, clippy::expect_used)]

use forge_cli::send::{run_for_test, AgentRecord, CommandOutput, InMemorySendBackend, SendBackend};

// ---------------------------------------------------------------------------
// Golden tests – human output
// ---------------------------------------------------------------------------

#[test]
fn send_single_human_matches_golden() {
    let mut backend = single_agent();
    let out = run(
        &["send", "oracle-agent-idle", "hello from oracle"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/send/single_human.txt"));
}

#[test]
fn send_when_idle_human_matches_golden() {
    let mut backend = single_agent();
    let out = run(
        &[
            "send",
            "oracle-agent-idle",
            "continue when ready",
            "--when-idle",
        ],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/send/when_idle_human.txt"));
}

#[test]
fn send_priority_high_human_matches_golden() {
    let mut backend = single_agent();
    let out = run(
        &["send", "--priority", "high", "oracle-agent-idle", "urgent"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(
        out.stdout,
        include_str!("golden/send/priority_high_human.txt")
    );
}

// ---------------------------------------------------------------------------
// JSON output structure tests
// ---------------------------------------------------------------------------

#[test]
fn send_json_has_correct_structure() {
    let mut backend = single_agent();
    let out = run(
        &["send", "oracle-agent-idle", "hello from oracle", "--json"],
        &mut backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["queued"], true);
    assert_eq!(parsed["message"], "hello from oracle");
    let results = parsed["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["agent_id"], "oracle-agent-idle");
    assert_eq!(results[0]["item_type"], "message");
    assert_eq!(results[0]["position"], 1);
    assert!(results[0]["item_id"].as_str().is_some());
}

#[test]
fn send_when_idle_json_has_conditional_type() {
    let mut backend = single_agent();
    let out = run(
        &[
            "send",
            "oracle-agent-idle",
            "continue when ready",
            "--when-idle",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["results"][0]["item_type"], "conditional");
    assert_eq!(parsed["results"][0]["position"], 1);
}

#[test]
fn send_jsonl_single_line() {
    let mut backend = single_agent();
    let out = run(
        &["send", "oracle-agent-idle", "hello", "--jsonl"],
        &mut backend,
    );
    assert_success(&out);
    let lines: Vec<&str> = out.stdout.trim().lines().collect();
    assert_eq!(lines.len(), 1);
    let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(parsed["queued"], true);
}

// ---------------------------------------------------------------------------
// All agents
// ---------------------------------------------------------------------------

#[test]
fn send_all_targets_every_agent() {
    let mut backend = multi_agent();
    let out = run(
        &["send", "--all", "broadcast message", "--json"],
        &mut backend,
    );
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    let results = parsed["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
}

// ---------------------------------------------------------------------------
// Auto-detect
// ---------------------------------------------------------------------------

#[test]
fn send_auto_detect_single_agent() {
    let mut backend = single_agent();
    let out = run(&["send", "hello auto", "--json"], &mut backend);
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["results"][0]["agent_id"], "oracle-agent-idle");
    assert_eq!(parsed["message"], "hello auto");
}

#[test]
fn send_context_agent_fallback() {
    let mut backend = multi_agent().with_context("oracle-agent-busy");
    let out = run(&["send", "hello ctx", "--json"], &mut backend);
    assert_success(&out);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["results"][0]["agent_id"], "oracle-agent-busy");
}

// ---------------------------------------------------------------------------
// Integration scenario
// ---------------------------------------------------------------------------

#[test]
fn send_integration_scenario() {
    let mut backend = single_agent();

    // Step 1: queue a normal message.
    let one = run(
        &["send", "oracle-agent-idle", "first message", "--json"],
        &mut backend,
    );
    assert_success(&one);
    let one_parsed: serde_json::Value = serde_json::from_str(&one.stdout).unwrap();
    assert_eq!(one_parsed["results"][0]["position"], 1);

    // Step 2: queue a when-idle conditional.
    let two = run(
        &[
            "send",
            "oracle-agent-idle",
            "when idle msg",
            "--when-idle",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&two);
    let two_parsed: serde_json::Value = serde_json::from_str(&two.stdout).unwrap();
    assert_eq!(two_parsed["results"][0]["position"], 2);
    assert_eq!(two_parsed["results"][0]["item_type"], "conditional");

    // Step 3: queue a high priority message (should go to front).
    let three = run(
        &[
            "send",
            "--priority",
            "high",
            "oracle-agent-idle",
            "urgent",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&three);
    let three_parsed: serde_json::Value = serde_json::from_str(&three.stdout).unwrap();
    assert_eq!(three_parsed["results"][0]["position"], 1);
    assert_eq!(three_parsed["results"][0]["item_type"], "message");
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn send_no_agents_errors() {
    let mut backend = InMemorySendBackend::default();
    let out = run(&["send", "hello"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("no agents in workspace"));
}

#[test]
fn send_empty_message_errors() {
    let mut backend = single_agent();
    let out = run(&["send", "oracle-agent-idle"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("message required"));
}

#[test]
fn send_multi_agent_no_context_errors() {
    let mut backend = multi_agent();
    let out = run(&["send", "hello"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("agent required"));
}

#[test]
fn send_unknown_flag_errors() {
    let mut backend = single_agent();
    let out = run(
        &["send", "--badarg", "oracle-agent-idle", "hello"],
        &mut backend,
    );
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("unknown flag"));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn single_agent() -> InMemorySendBackend {
    InMemorySendBackend::with_agents(vec![AgentRecord {
        id: "oracle-agent-idle".to_string(),
        workspace_id: "ws-001".to_string(),
        state: "idle".to_string(),
    }])
}

fn multi_agent() -> InMemorySendBackend {
    InMemorySendBackend::with_agents(vec![
        AgentRecord {
            id: "oracle-agent-idle".to_string(),
            workspace_id: "ws-001".to_string(),
            state: "idle".to_string(),
        },
        AgentRecord {
            id: "oracle-agent-busy".to_string(),
            workspace_id: "ws-001".to_string(),
            state: "working".to_string(),
        },
    ])
}

fn run(args: &[&str], backend: &mut dyn SendBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(out: &CommandOutput) {
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
}
