#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_cli::context::{
    run_context_for_test, run_use_for_test, CommandOutput, ContextBackend, ContextRecord,
    InMemoryContextBackend, WorkspaceInfo,
};

// ---------------------------------------------------------------------------
// Golden file tests for `forge context`
// ---------------------------------------------------------------------------

#[test]
fn context_empty_outputs_match_goldens() {
    let backend = InMemoryContextBackend::default();

    let human = run_context(&["context"], &backend);
    assert_success(&human);
    assert_eq!(human.stdout, include_str!("golden/context/empty.txt"));

    let json = run_context(&["context", "--json"], &backend);
    assert_success(&json);
    assert_eq!(json.stdout, include_str!("golden/context/empty.json"));
}

#[test]
fn context_non_empty_outputs_match_goldens() {
    let backend = InMemoryContextBackend {
        context: std::cell::RefCell::new(ContextRecord {
            workspace_id: "ws_123456789".to_string(),
            workspace_name: "myws".to_string(),
            agent_id: "agent_abcdef1234567890".to_string(),
            agent_name: "agname".to_string(),
            updated_at: "2026-02-09T12:00:00Z".to_string(),
        }),
        ..Default::default()
    };

    let human = run_context(&["context"], &backend);
    assert_success(&human);
    assert_eq!(human.stdout, include_str!("golden/context/nonempty.txt"));

    let json = run_context(&["context", "--json"], &backend);
    assert_success(&json);
    assert_eq!(json.stdout, include_str!("golden/context/nonempty.json"));

    let jsonl = run_context(&["context", "--jsonl"], &backend);
    assert_success(&jsonl);
    assert_eq!(jsonl.stdout, include_str!("golden/context/nonempty.jsonl"));
}

#[test]
fn context_error_paths() {
    let backend = InMemoryContextBackend::default();

    let unknown = run_context(&["context", "--bogus"], &backend);
    assert_eq!(unknown.exit_code, 1);
    assert!(unknown.stdout.is_empty());
    assert_eq!(unknown.stderr, "unknown flag: --bogus\n");
}

// ---------------------------------------------------------------------------
// Integration tests for `forge use`
// ---------------------------------------------------------------------------

#[test]
fn use_empty_show() {
    let backend = InMemoryContextBackend::default();
    let out = run_use(&["use"], &backend);
    assert_success(&out);
    assert!(out.stdout.contains("No context set."));
}

#[test]
fn use_set_workspace_and_verify() {
    let backend = InMemoryContextBackend {
        workspaces: vec![WorkspaceInfo {
            id: "ws_abc12345".to_string(),
            name: "my-project".to_string(),
        }],
        ..Default::default()
    };
    let out = run_use(&["use", "my-project"], &backend);
    assert_success(&out);
    assert!(out.stdout.contains("Workspace set to: my-project"));

    // Verify context was saved
    let show = run_context(&["context"], &backend);
    assert_success(&show);
    assert_eq!(show.stdout, "Context: my-project\n");
}

#[test]
fn use_clear_and_verify() {
    let backend = InMemoryContextBackend {
        context: std::cell::RefCell::new(ContextRecord {
            workspace_id: "ws_abc".to_string(),
            workspace_name: "project".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };

    let out = run_use(&["use", "--clear"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, "Context cleared.\n");

    let show = run_context(&["context"], &backend);
    assert_success(&show);
    assert_eq!(show.stdout, "No context set.\n");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn run_context(args: &[&str], backend: &dyn ContextBackend) -> CommandOutput {
    run_context_for_test(args, backend)
}

fn run_use(args: &[&str], backend: &dyn ContextBackend) -> CommandOutput {
    run_use_for_test(args, backend)
}

fn assert_success(output: &CommandOutput) {
    assert_eq!(output.exit_code, 0);
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        output.stderr
    );
}
