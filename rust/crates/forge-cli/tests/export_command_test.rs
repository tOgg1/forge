use forge_cli::export::{
    run_for_test, CommandOutput, ExportAgent, ExportAlert, ExportEvent, ExportNode,
    ExportQueueItem, ExportStatus, ExportWorkspace, InMemoryExportBackend,
};

#[test]
fn export_status_json_schema_contains_expected_keys() {
    let backend = seeded_backend();
    let out = run(&["export", "status", "--json"], &backend);
    assert_success(&out);

    let parsed: serde_json::Value =
        serde_json::from_str(&out.stdout).unwrap_or_else(|err| panic!("parse json: {err}"));
    assert!(parsed.get("nodes").is_some());
    assert!(parsed.get("workspaces").is_some());
    assert!(parsed.get("agents").is_some());
    assert!(parsed.get("queues").is_some());
    assert!(parsed.get("alerts").is_some());
    assert_eq!(parsed["nodes"].as_array().map(|items| items.len()), Some(1));
    assert_eq!(
        parsed["workspaces"].as_array().map(|items| items.len()),
        Some(1)
    );
    assert_eq!(
        parsed["agents"].as_array().map(|items| items.len()),
        Some(1)
    );
    assert_eq!(
        parsed["queues"].as_array().map(|items| items.len()),
        Some(1)
    );
    assert_eq!(
        parsed["alerts"].as_array().map(|items| items.len()),
        Some(1)
    );
}

#[test]
fn export_events_jsonl_filters_by_type_and_agent() {
    let backend = seeded_backend();
    let out = run(
        &[
            "export",
            "events",
            "--jsonl",
            "--type",
            "agent.state_changed,message.dispatched",
            "--agent",
            "agent-1",
        ],
        &backend,
    );
    assert_success(&out);

    let lines: Vec<&str> = out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 3, "stdout: {}", out.stdout);
    for line in lines {
        let row: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|err| panic!("parse jsonl row: {err}"));
        assert_eq!(row["entity_id"], "agent-1");
        let event_type = row["type"]
            .as_str()
            .unwrap_or_else(|| panic!("type should be string"));
        assert!(matches!(
            event_type,
            "agent.state_changed" | "message.dispatched"
        ));
    }
}

#[test]
fn export_events_filters_since_until_window() {
    let backend = seeded_backend();
    let out = run(
        &[
            "export",
            "events",
            "--json",
            "--since",
            "2026-01-01T00:00:05Z",
            "--until",
            "2026-01-01T00:00:15Z",
        ],
        &backend,
    );
    assert_success(&out);

    let payload: serde_json::Value =
        serde_json::from_str(&out.stdout).unwrap_or_else(|err| panic!("parse json: {err}"));
    let events = payload
        .as_array()
        .unwrap_or_else(|| panic!("events should be array"));
    assert_eq!(events.len(), 3, "stdout: {}", out.stdout);
    for event in events {
        let timestamp = event["timestamp"]
            .as_str()
            .unwrap_or_else(|| panic!("timestamp should be string"));
        assert!(timestamp >= "2026-01-01T00:00:05Z");
        assert!(timestamp <= "2026-01-01T00:00:15Z");
    }
}

fn seeded_backend() -> InMemoryExportBackend {
    InMemoryExportBackend::default()
        .with_status(ExportStatus {
            nodes: vec![ExportNode {
                id: "node-1".to_string(),
                name: "local".to_string(),
                status: "online".to_string(),
                ssh_target: None,
                is_local: true,
                agent_count: 1,
            }],
            workspaces: vec![ExportWorkspace {
                id: "ws-1".to_string(),
                name: "forge".to_string(),
                node_id: "node-1".to_string(),
                status: "active".to_string(),
                agent_count: 1,
                alerts: vec![ExportAlert {
                    alert_type: "queue_backlog".to_string(),
                    severity: "warning".to_string(),
                    message: "pending".to_string(),
                    agent_id: Some("agent-1".to_string()),
                }],
            }],
            agents: vec![ExportAgent {
                id: "agent-1".to_string(),
                workspace_id: "ws-1".to_string(),
                state: "running".to_string(),
                agent_type: "codex".to_string(),
                queue_length: 1,
            }],
            queues: vec![ExportQueueItem {
                id: "q-1".to_string(),
                agent_id: "agent-1".to_string(),
                item_type: "message_append".to_string(),
                position: 1,
                status: "pending".to_string(),
            }],
            alerts: vec![ExportAlert {
                alert_type: "queue_backlog".to_string(),
                severity: "warning".to_string(),
                message: "pending".to_string(),
                agent_id: Some("agent-1".to_string()),
            }],
        })
        .with_events(vec![
            ExportEvent {
                id: "evt-001".to_string(),
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                event_type: "agent.state_changed".to_string(),
                entity_type: "agent".to_string(),
                entity_id: "agent-1".to_string(),
                payload: Some(serde_json::json!({"state":"starting"})),
                metadata: None,
            },
            ExportEvent {
                id: "evt-002".to_string(),
                timestamp: "2026-01-01T00:00:10Z".to_string(),
                event_type: "message.dispatched".to_string(),
                entity_type: "agent".to_string(),
                entity_id: "agent-1".to_string(),
                payload: Some(serde_json::json!({"queue":"q-1"})),
                metadata: None,
            },
            ExportEvent {
                id: "evt-003".to_string(),
                timestamp: "2026-01-01T00:00:12Z".to_string(),
                event_type: "agent.state_changed".to_string(),
                entity_type: "agent".to_string(),
                entity_id: "agent-2".to_string(),
                payload: Some(serde_json::json!({"state":"idle"})),
                metadata: None,
            },
            ExportEvent {
                id: "evt-004".to_string(),
                timestamp: "2026-01-01T00:00:15Z".to_string(),
                event_type: "agent.state_changed".to_string(),
                entity_type: "agent".to_string(),
                entity_id: "agent-1".to_string(),
                payload: Some(serde_json::json!({"state":"running"})),
                metadata: None,
            },
            ExportEvent {
                id: "evt-005".to_string(),
                timestamp: "2026-01-01T00:00:20Z".to_string(),
                event_type: "warning".to_string(),
                entity_type: "system".to_string(),
                entity_id: "daemon".to_string(),
                payload: None,
                metadata: None,
            },
        ])
}

fn run(args: &[&str], backend: &InMemoryExportBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(out: &CommandOutput) {
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
}
