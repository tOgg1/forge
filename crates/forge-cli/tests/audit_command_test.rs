use forge_cli::audit::{
    run_for_test, AuditBackend, AuditEvent, CommandOutput, InMemoryAuditBackend,
};

#[test]
fn audit_table_default_matches_golden() {
    let backend = seeded();
    let out = run(&["audit"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/audit/default_table.txt"));
}

#[test]
fn audit_json_filtered_matches_golden() {
    let backend = seeded();
    let out = run(
        &[
            "audit",
            "--json",
            "--type",
            "agent.state_changed",
            "--entity-type",
            "agent",
        ],
        &backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/audit/filtered.json"));
}

#[test]
fn audit_jsonl_matches_golden() {
    let backend = seeded();
    let out = run(
        &[
            "audit",
            "--jsonl",
            "--action",
            "message.dispatched",
            "--limit",
            "1",
        ],
        &backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/audit/action.jsonl"));
}

#[test]
fn audit_rejects_type_and_action_combination() {
    let backend = seeded();
    let out = run(
        &[
            "audit",
            "--type",
            "agent.state_changed",
            "--action",
            "message.dispatched",
        ],
        &backend,
    );
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "use either --type or --action, not both\n");
}

#[test]
fn audit_rejects_since_after_until() {
    let backend = seeded();
    let out = run(
        &[
            "audit",
            "--since",
            "2026-01-01T00:00:30Z",
            "--until",
            "2026-01-01T00:00:00Z",
        ],
        &backend,
    );
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "--since must be before --until\n");
}

#[test]
fn audit_oracle_flow_matches_fixture() {
    let backend = seeded();
    let mut steps = Vec::new();

    let default_table = run(&["audit"], &backend);
    steps.push(OracleStep {
        name: "audit table".to_string(),
        stdout: default_table.stdout,
        stderr: default_table.stderr,
        exit_code: default_table.exit_code,
    });

    let filtered_json = run(
        &[
            "audit",
            "--json",
            "--type",
            "agent.state_changed",
            "--entity-type",
            "agent",
        ],
        &backend,
    );
    steps.push(OracleStep {
        name: "audit filtered json".to_string(),
        stdout: filtered_json.stdout,
        stderr: filtered_json.stderr,
        exit_code: filtered_json.exit_code,
    });

    let missing_combo = run(
        &[
            "audit",
            "--type",
            "agent.state_changed",
            "--action",
            "message.dispatched",
        ],
        &backend,
    );
    steps.push(OracleStep {
        name: "audit invalid type+action".to_string(),
        stdout: missing_combo.stdout,
        stderr: missing_combo.stderr,
        exit_code: missing_combo.exit_code,
    });

    let unknown = run(&["audit", "--bogus"], &backend);
    steps.push(OracleStep {
        name: "audit unknown flag".to_string(),
        stdout: unknown.stdout,
        stderr: unknown.stderr,
        exit_code: unknown.exit_code,
    });

    let report = OracleReport { steps };
    let got = match serde_json::to_string_pretty(&report) {
        Ok(text) => text + "\n",
        Err(err) => panic!("failed to encode report: {err}"),
    };

    let path = format!("{}/testdata/audit_oracle.json", env!("CARGO_MANIFEST_DIR"));
    let want = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => panic!("failed to read {path}: {err}"),
    };

    assert_eq!(want.replace("\r\n", "\n"), got.replace("\r\n", "\n"));
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct OracleReport {
    steps: Vec<OracleStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct OracleStep {
    name: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
}

fn seeded() -> InMemoryAuditBackend {
    InMemoryAuditBackend::with_events(vec![
        AuditEvent {
            id: "evt-001".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            event_type: "agent.state_changed".to_string(),
            entity_type: "agent".to_string(),
            entity_id: "agent-1".to_string(),
            payload: r#"{"state":"running"}"#.to_string(),
            metadata: Some(std::collections::BTreeMap::from([(
                "source".to_string(),
                "loop".to_string(),
            )])),
        },
        AuditEvent {
            id: "evt-002".to_string(),
            timestamp: "2026-01-01T00:00:10Z".to_string(),
            event_type: "message.dispatched".to_string(),
            entity_type: "queue".to_string(),
            entity_id: "queue-1".to_string(),
            payload: r#"{"message":"ship"}"#.to_string(),
            metadata: None,
        },
        AuditEvent {
            id: "evt-003".to_string(),
            timestamp: "2026-01-01T00:00:20Z".to_string(),
            event_type: "warning".to_string(),
            entity_type: "system".to_string(),
            entity_id: "core".to_string(),
            payload: String::new(),
            metadata: None,
        },
        AuditEvent {
            id: "evt-004".to_string(),
            timestamp: "2026-01-01T00:00:30Z".to_string(),
            event_type: "agent.state_changed".to_string(),
            entity_type: "agent".to_string(),
            entity_id: "agent-2".to_string(),
            payload: r#"{"state":"sleeping"}"#.to_string(),
            metadata: Some(std::collections::BTreeMap::from([(
                "source".to_string(),
                "daemon".to_string(),
            )])),
        },
    ])
}

fn run(args: &[&str], backend: &dyn AuditBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(out: &CommandOutput) {
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
}
