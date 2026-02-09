#![allow(clippy::unwrap_used)]

use forge_cli::{run_for_test, RootCommandOutput};

// -- Help ----------------------------------------------------------------

#[test]
fn root_no_args_shows_help() {
    let out = run(&[]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert!(out.stdout.contains("Control plane for AI coding agents"));
    assert!(out.stdout.contains("Commands:"));
    assert!(out.stdout.contains("Global Flags:"));
}

#[test]
fn root_help_flag() {
    let out = run(&["--help"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert!(out.stdout.contains("Control plane for AI coding agents"));
    assert!(out.stdout.contains("Commands:"));
}

#[test]
fn root_dash_h_flag() {
    let out = run(&["-h"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("Commands:"));
}

#[test]
fn root_help_subcommand() {
    let out = run(&["help"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("Commands:"));
}

// -- Version -------------------------------------------------------------

#[test]
fn root_version_flag() {
    let out = run(&["--version"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.starts_with("forge version "));
    assert!(out.stderr.is_empty());
}

#[test]
fn root_version_contains_commit_info() {
    let out = run(&["--version"]);
    // Default version string includes (commit: ..., built: ...)
    assert!(out.stdout.contains("commit:"));
    assert!(out.stdout.contains("built:"));
}

// -- Unknown command: text mode ------------------------------------------

#[test]
fn unknown_command_text_error() {
    let out = run(&["nonexistent"]);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("unknown forge command: nonexistent"));
    // Help text is also printed to stderr
    assert!(out.stderr.contains("Commands:"));
}

// -- Unknown command: JSON mode ------------------------------------------

#[test]
fn unknown_command_json_error_matches_golden() {
    let out = run(&["--json", "nonexistent"]);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.is_empty());
    assert_eq!(
        out.stdout,
        include_str!("golden/root/unknown_command_error.json")
    );
}

#[test]
fn unknown_command_jsonl_error_matches_golden() {
    let out = run(&["--jsonl", "nonexistent"]);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.is_empty());
    assert_eq!(
        out.stdout,
        include_str!("golden/root/unknown_command_error.jsonl")
    );
}

#[test]
fn unknown_command_json_no_help_on_stdout() {
    let out = run(&["--json", "nonexistent"]);
    // In JSON mode, help text should NOT be printed (neither stdout nor stderr)
    assert!(!out.stdout.contains("Commands:"));
    assert!(out.stderr.is_empty());
}

// -- Global flag forwarding ----------------------------------------------

#[test]
fn global_verbose_quiet_before_help() {
    let out = run(&["--verbose", "--quiet", "--help"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.contains("Commands:"));
}

#[test]
fn global_json_before_version() {
    // --version should take precedence even when --json is also present
    let out = run(&["--json", "--version"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stdout.starts_with("forge version "));
}

// -- Error envelope classification (integration) -------------------------

#[test]
fn json_error_envelope_ambiguous() {
    // Trigger an ambiguous-like error message through unknown command containing "ambiguous"
    // (The error classification works on message content, not on the actual cause.)
    let out = run(&["--json", "ambiguous-prefix"]);
    assert_eq!(out.exit_code, 1);
    let parsed: serde_json::Value = match serde_json::from_str(&out.stdout) {
        Ok(value) => value,
        Err(err) => panic!("expected valid json envelope: {err}"),
    };
    // "unknown forge command: ambiguous-prefix" contains "ambiguous"
    assert_eq!(parsed["error"]["code"], "ERR_AMBIGUOUS");
}

// -- Helper --------------------------------------------------------------

fn run(args: &[&str]) -> RootCommandOutput {
    run_for_test(args)
}
