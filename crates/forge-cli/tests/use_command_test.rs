#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_cli::context::{run_use_for_test, ContextRecord, InMemoryContextBackend};

#[test]
fn use_empty_show_matches_golden() {
    let backend = InMemoryContextBackend::default();
    let out = run_use_for_test(&["use"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/use/empty_show.txt"));
}

#[test]
fn use_non_empty_show_matches_golden() {
    let backend = InMemoryContextBackend {
        context: std::cell::RefCell::new(ContextRecord {
            workspace_id: "ws_123456789".to_string(),
            workspace_name: "myws".to_string(),
            agent_id: "agent_abcdef1234567890".to_string(),
            agent_name: "agent_abc".to_string(),
            updated_at: "2026-02-09T12:00:00Z".to_string(),
        }),
        ..Default::default()
    };

    let out = run_use_for_test(&["use", "--show"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/use/nonempty_show.txt"));
}
