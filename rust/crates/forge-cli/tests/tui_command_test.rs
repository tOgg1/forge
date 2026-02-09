#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_cli::tui::{run_for_test, InMemoryTuiBackend};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn interactive_backend() -> InMemoryTuiBackend {
    InMemoryTuiBackend::default()
}

fn non_interactive_backend() -> InMemoryTuiBackend {
    InMemoryTuiBackend {
        non_interactive: true,
        ..Default::default()
    }
}

fn assert_success(out: &forge_cli::tui::CommandOutput) {
    assert_eq!(
        out.exit_code, 0,
        "expected exit 0, got {}: stdout={} stderr={}",
        out.exit_code, out.stdout, out.stderr
    );
}

// ---------------------------------------------------------------------------
// Help
// ---------------------------------------------------------------------------

#[test]
fn tui_help_matches_golden() {
    let backend = interactive_backend();
    let out = run_for_test(&["tui", "--help"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/tui/help.txt"));
}

#[test]
fn ui_alias_help_matches_golden() {
    let backend = interactive_backend();
    let out = run_for_test(&["ui", "--help"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/tui/help.txt"));
}

// ---------------------------------------------------------------------------
// Launch (interactive)
// ---------------------------------------------------------------------------

#[test]
fn tui_launch_succeeds_silently() {
    let backend = interactive_backend();
    let out = run_for_test(&["tui"], &backend);
    assert_success(&out);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty());
    assert!(backend.launched.get());
}

#[test]
fn tui_launch_json_matches_golden() {
    let backend = interactive_backend();
    let out = run_for_test(&["tui", "--json"], &backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/tui/launch_ok_json.json"));
}

#[test]
fn ui_alias_launches() {
    let backend = interactive_backend();
    let out = run_for_test(&["ui"], &backend);
    assert_success(&out);
    assert!(backend.launched.get());
}

// ---------------------------------------------------------------------------
// Non-interactive errors
// ---------------------------------------------------------------------------

#[test]
fn tui_non_interactive_text_matches_golden() {
    let backend = non_interactive_backend();
    let out = run_for_test(&["tui"], &backend);
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        out.stderr,
        include_str!("golden/tui/non_interactive_text.txt")
    );
    assert!(out.stdout.is_empty());
    assert!(!backend.launched.get());
}

#[test]
fn tui_non_interactive_json_matches_golden() {
    let backend = non_interactive_backend();
    let out = run_for_test(&["tui", "--json"], &backend);
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        out.stdout,
        include_str!("golden/tui/non_interactive_json.json")
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn tui_non_interactive_jsonl_matches_golden() {
    let backend = non_interactive_backend();
    let out = run_for_test(&["tui", "--jsonl"], &backend);
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        out.stdout,
        include_str!("golden/tui/non_interactive_jsonl.jsonl")
    );
    assert!(out.stderr.is_empty());
}

// ---------------------------------------------------------------------------
// Launch errors
// ---------------------------------------------------------------------------

#[test]
fn tui_launch_error_text() {
    let backend = InMemoryTuiBackend {
        launch_error: Some("failed to open database".to_string()),
        ..Default::default()
    };
    let out = run_for_test(&["tui"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("failed to open database"));
    assert!(backend.launched.get());
}

#[test]
fn tui_launch_error_json() {
    let backend = InMemoryTuiBackend {
        launch_error: Some("failed to open database".to_string()),
        ..Default::default()
    };
    let out = run_for_test(&["tui", "--json"], &backend);
    assert_eq!(out.exit_code, 2);
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["error"]["code"], "ERR_OPERATION_FAILED");
    assert_eq!(parsed["error"]["message"], "failed to open database");
    assert!(out.stderr.is_empty());
}

// ---------------------------------------------------------------------------
// Invalid flags
// ---------------------------------------------------------------------------

#[test]
fn tui_unknown_flag_returns_error() {
    let backend = interactive_backend();
    let out = run_for_test(&["tui", "--bogus"], &backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("unknown flag: --bogus"));
}
