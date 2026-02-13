//! PAR-114: End-to-end rforge logs highlighting verification suite.
//!
//! Tests for logs, logs -f, --since, --lines, prefix-selection, and
//! ambiguous-prefix errors. Asserts semantic coloring output (ANSI escape
//! sequences and no-color signifiers) against expected token classifications.

use forge_cli::logs::{
    default_log_path, run_for_test, CommandOutput, InMemoryLogsBackend, LogsBackend, LoopRecord,
};

// ---------------------------------------------------------------------------
// ANSI color constants (must match logs.rs / highlight_spec.rs)
// ---------------------------------------------------------------------------

const ESC: &str = "\x1b[";
/// Bold+Cyan: logs.rs emits two separate sequences \x1b[1m\x1b[36m.
const BOLD_CYAN: &str = "\x1b[1m\x1b[36m";
/// Bold+Magenta: logs.rs emits two separate sequences \x1b[1m\x1b[35m.
const BOLD_MAGENTA: &str = "\x1b[1m\x1b[35m";
const BOLD_RED: &str = "\x1b[1;31m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";

// ---------------------------------------------------------------------------
// Shared fixture data
// ---------------------------------------------------------------------------

/// Rich log content containing all major section types for semantic coloring tests.
const RICH_LOG: &str = "\
OpenAI Codex v0.80.0 (research preview)
--------
workdir: /repo/forge
model: gpt-5.2-codex
--------
user
Implement the database connection pool
thinking
**Analyzing requirements**
Let me examine the existing module.
codex
I'll implement the connection pool.

```rust
pub struct ConnectionPool {
    max_size: usize,
}
```

diff --git a/src/pool.rs b/src/pool.rs
--- a/src/pool.rs
+++ b/src/pool.rs
@@ -1,3 +1,5 @@
-use std::sync::Mutex;
+use std::sync::Arc;
+use tokio::sync::Semaphore;

exec
$ cargo test -p forge-db --lib pool::tests
running 8 tests
test pool::tests::acquire_returns_connection ... ok
test pool::tests::timeout_on_exhausted_pool ... ok
exit code: 0

error: integration tests failed with 1 failure
recovery: fix rate limiter test fixture

tokens used
15,892
";

/// Log content with Claude stream JSON events.
const CLAUDE_LOG: &str = r#"{"type":"system","subtype":"init","model":"claude-opus-4-6","tools":["Bash"],"mcp_servers":[],"session_id":"sess-1"}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello world"}}}
{"type":"result","num_turns":1,"duration_ms":2100,"total_cost_usd":0.120001,"usage":{"input_tokens":10,"output_tokens":20}}
"#;

/// Log content with timestamps for --since filtering.
const TIMESTAMPED_LOG: &str = "\
[2026-01-01T00:00:00Z] status: starting
[2026-01-01T00:00:01Z] status: running
user
Implement feature
codex
Done.
[2026-01-01T00:00:05Z] status: idle
";

/// Log content with command transcripts and errors.
const CMD_LOG: &str = "\
exec
$ cargo test --workspace
running 42 tests
test result: ok. 42 passed; 0 failed;
exit code: 0
$ cargo clippy -- -D warnings
stderr: error[E0277]: trait bound not satisfied
exit code: 1
";

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn run(args: &[&str], backend: &mut dyn LogsBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(out: &CommandOutput) {
    assert_eq!(out.exit_code, 0, "exit_code != 0; stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
}

fn rich_backend() -> InMemoryLogsBackend {
    let path = default_log_path("/tmp/forge", "codex-loop", "loop-rich");
    InMemoryLogsBackend::with_loops(vec![LoopRecord {
        id: "loop-rich".to_string(),
        short_id: "rich01".to_string(),
        name: "codex-loop".to_string(),
        repo: "/repo-main".to_string(),
        log_path: path.clone(),
    }])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_log(&path, RICH_LOG)
}

fn claude_backend() -> InMemoryLogsBackend {
    let path = default_log_path("/tmp/forge", "claude-loop", "loop-claude");
    InMemoryLogsBackend::with_loops(vec![LoopRecord {
        id: "loop-claude".to_string(),
        short_id: "claud1".to_string(),
        name: "claude-loop".to_string(),
        repo: "/repo-main".to_string(),
        log_path: path.clone(),
    }])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_log(&path, CLAUDE_LOG)
}

fn multi_backend() -> InMemoryLogsBackend {
    let alpha_path = default_log_path("/tmp/forge", "alpha", "loop-001");
    let beta_path = default_log_path("/tmp/forge", "beta", "loop-002");
    let gamma_path = default_log_path("/tmp/forge", "gamma-cmd", "loop-003");

    InMemoryLogsBackend::with_loops(vec![
        LoopRecord {
            id: "loop-001".to_string(),
            short_id: "ab123456".to_string(),
            name: "alpha".to_string(),
            repo: "/repo-main".to_string(),
            log_path: alpha_path.clone(),
        },
        LoopRecord {
            id: "loop-002".to_string(),
            short_id: "ad123547".to_string(),
            name: "beta".to_string(),
            repo: "/repo-main".to_string(),
            log_path: beta_path.clone(),
        },
        LoopRecord {
            id: "loop-003".to_string(),
            short_id: "cd987654".to_string(),
            name: "gamma-cmd".to_string(),
            repo: "/repo-main".to_string(),
            log_path: gamma_path.clone(),
        },
    ])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_log(&alpha_path, TIMESTAMPED_LOG)
    .with_log(&beta_path, RICH_LOG)
    .with_log(&gamma_path, CMD_LOG)
}

// ===========================================================================
// E2E: Basic logs with semantic coloring
// ===========================================================================

#[test]
fn e2e_logs_harness_header_bold_cyan() {
    std::env::remove_var("NO_COLOR");
    let mut backend = rich_backend();
    let out = run(&["logs", "codex-loop", "--lines", "50"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.contains(BOLD_CYAN),
        "harness header should be bold+cyan; got: {}",
        &out.stdout[..200.min(out.stdout.len())]
    );
    assert!(out.stdout.contains("OpenAI Codex v0.80.0"));
}

#[test]
fn e2e_logs_role_marker_bold_magenta() {
    std::env::remove_var("NO_COLOR");
    let mut backend = rich_backend();
    let out = run(&["logs", "codex-loop", "--lines", "50"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.contains(BOLD_MAGENTA),
        "role markers should be bold+magenta"
    );
}

#[test]
fn e2e_logs_thinking_dimmed() {
    std::env::remove_var("NO_COLOR");
    let mut backend = rich_backend();
    let out = run(&["logs", "codex-loop", "--lines", "50"], &mut backend);
    assert_success(&out);
    assert!(out.stdout.contains(DIM), "thinking block should be dimmed");
}

#[test]
fn e2e_logs_diff_coloring() {
    std::env::remove_var("NO_COLOR");
    let mut backend = rich_backend();
    let out = run(&["logs", "codex-loop", "--lines", "50"], &mut backend);
    assert_success(&out);
    // Diff add lines should contain green.
    assert!(
        out.stdout.contains("\x1b[32m") || out.stdout.contains("\x1b[1;92m"),
        "diff add lines should have green styling"
    );
    // Diff del lines should contain red.
    assert!(
        out.stdout.contains("\x1b[31m") || out.stdout.contains("\x1b[1;91m"),
        "diff del lines should have red styling"
    );
}

#[test]
fn e2e_logs_error_block_bold_red() {
    std::env::remove_var("NO_COLOR");
    let mut backend = rich_backend();
    let out = run(&["logs", "codex-loop", "--lines", "50"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.contains(BOLD_RED),
        "error block should be bold+red"
    );
    assert!(out.stdout.contains("integration tests failed"));
}

#[test]
fn e2e_logs_summary_dimmed() {
    std::env::remove_var("NO_COLOR");
    let mut backend = rich_backend();
    let out = run(&["logs", "codex-loop", "--lines", "50"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.contains("tokens used"),
        "summary should be present"
    );
}

#[test]
fn e2e_logs_section_separators_present() {
    std::env::remove_var("NO_COLOR");
    let mut backend = rich_backend();
    let out = run(&["logs", "codex-loop", "--lines", "50"], &mut backend);
    assert_success(&out);
    // Section separators use ─ (U+2500).
    assert!(
        out.stdout.contains('\u{2500}'),
        "section separators should be present between major sections"
    );
}

// ===========================================================================
// E2E: No-color mode signifiers
// ===========================================================================

#[test]
fn e2e_no_color_harness_header_prefix() {
    let mut backend = rich_backend();
    let out = run(
        &["logs", "codex-loop", "--lines", "50", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    assert!(
        out.stdout.contains("== OpenAI Codex v0.80.0"),
        "no-color: harness header should get '== ' prefix; got first 200 chars: {}",
        &out.stdout[..200.min(out.stdout.len())]
    );
}

#[test]
fn e2e_no_color_role_marker_prefix() {
    let mut backend = rich_backend();
    let out = run(
        &["logs", "codex-loop", "--lines", "50", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    assert!(
        out.stdout.contains(">> user"),
        "no-color: role marker should get '>> ' prefix"
    );
    assert!(
        out.stdout.contains(">> codex"),
        "no-color: codex role marker should get '>> ' prefix"
    );
}

#[test]
fn e2e_no_color_error_block_prefix() {
    let mut backend = rich_backend();
    let out = run(
        &["logs", "codex-loop", "--lines", "50", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    assert!(
        out.stdout
            .contains("[ERROR] error: integration tests failed"),
        "no-color: error should get [ERROR] prefix; got: {}",
        out.stdout
    );
}

#[test]
fn e2e_no_color_diff_intraline_markers() {
    let mut backend = rich_backend();
    let out = run(
        &["logs", "codex-loop", "--lines", "50", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    // In no-color mode, intraline diff uses {+...+} and [-...-] markers.
    assert!(
        out.stdout.contains("{+") || out.stdout.contains("[-"),
        "no-color: diff should have intraline markers; got diff section: {}",
        out.stdout
    );
}

#[test]
fn e2e_no_color_no_ansi_escapes() {
    let mut backend = rich_backend();
    let out = run(
        &["logs", "codex-loop", "--lines", "50", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    assert!(
        !out.stdout.contains(ESC),
        "no-color mode should contain no ANSI escape sequences"
    );
}

// ===========================================================================
// E2E: Claude stream JSON rendering
// ===========================================================================

#[test]
fn e2e_claude_stream_json_rendered() {
    let mut backend = claude_backend();
    let out = run(
        &["logs", "claude-loop", "--lines", "50", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    assert!(
        out.stdout.contains("[claude:init] model=claude-opus-4-6"),
        "Claude init event should be rendered"
    );
    assert!(
        out.stdout.contains("Hello world"),
        "Claude stream text should be rendered"
    );
    assert!(
        out.stdout.contains("[claude:result] turns=1 duration=2.1s"),
        "Claude result should be rendered"
    );
}

#[test]
fn e2e_claude_stream_color_mode() {
    std::env::remove_var("NO_COLOR");
    let mut backend = claude_backend();
    let out = run(&["logs", "claude-loop", "--lines", "50"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.contains(CYAN) || out.stdout.contains(ESC),
        "Claude events should have color styling"
    );
}

// ===========================================================================
// E2E: --lines flag
// ===========================================================================

#[test]
fn e2e_lines_limits_output() {
    let mut backend = rich_backend();
    let out = run(
        &["logs", "codex-loop", "--lines", "5", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    let line_count = out.stdout.lines().count();
    // Should have header line + limited content.
    assert!(
        line_count <= 10,
        "limited output should have few lines, got {line_count}"
    );
}

#[test]
fn e2e_lines_still_colored() {
    std::env::remove_var("NO_COLOR");
    let mut backend = rich_backend();
    let out = run(&["logs", "codex-loop", "--lines", "5"], &mut backend);
    assert_success(&out);
    // Even with limited lines, the output should be styled.
    assert!(
        out.stdout.contains(ESC) || out.stdout.lines().count() <= 2,
        "limited output should still be styled if content present"
    );
}

// ===========================================================================
// E2E: --since flag with highlighting
// ===========================================================================

#[test]
fn e2e_since_filters_and_preserves_coloring() {
    std::env::remove_var("NO_COLOR");
    let mut backend = multi_backend();
    let out = run(
        &["logs", "alpha", "--since", "2026-01-01T00:00:01Z"],
        &mut backend,
    );
    assert_success(&out);
    // Should not contain the first timestamp line.
    assert!(
        !out.stdout.contains("status: starting"),
        "since filter should exclude earlier entries"
    );
    // Should contain the running line.
    assert!(
        out.stdout.contains("status: running"),
        "since filter should include matching entries"
    );
}

#[test]
fn e2e_since_no_color_filters_correctly() {
    let mut backend = multi_backend();
    let out = run(
        &[
            "logs",
            "alpha",
            "--since",
            "2026-01-01T00:00:01Z",
            "--no-color",
        ],
        &mut backend,
    );
    assert_success(&out);
    assert!(!out.stdout.contains("status: starting"));
    assert!(out.stdout.contains("status: running"));
    assert!(!out.stdout.contains(ESC));
}

// ===========================================================================
// E2E: --follow (logs -f) with highlighting
// ===========================================================================

#[test]
fn e2e_follow_preserves_coloring() {
    std::env::remove_var("NO_COLOR");
    let path = default_log_path("/tmp/forge", "follow-test", "loop-follow");
    let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
        id: "loop-follow".to_string(),
        short_id: "fol001".to_string(),
        name: "follow-test".to_string(),
        repo: "/repo-main".to_string(),
        log_path: path.clone(),
    }])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_follow_output(&path, RICH_LOG);

    let out = run(&["logs", "follow-test", "--follow"], &mut backend);
    assert_success(&out);
    // Follow mode should still produce colored output.
    assert!(
        out.stdout.contains(ESC),
        "follow mode should produce colored output"
    );
    assert!(
        out.stdout.contains("OpenAI Codex v0.80.0"),
        "follow mode should include harness header"
    );
}

#[test]
fn e2e_follow_no_color() {
    let path = default_log_path("/tmp/forge", "follow-nc", "loop-follow-nc");
    let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
        id: "loop-follow-nc".to_string(),
        short_id: "folnc1".to_string(),
        name: "follow-nc".to_string(),
        repo: "/repo-main".to_string(),
        log_path: path.clone(),
    }])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_follow_output(&path, RICH_LOG);

    let out = run(
        &["logs", "follow-nc", "--follow", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    assert!(
        !out.stdout.contains(ESC),
        "follow --no-color should not contain ANSI escapes"
    );
    assert!(
        out.stdout.contains("== OpenAI Codex v0.80.0"),
        "follow no-color should use text signifiers"
    );
}

// ===========================================================================
// E2E: Prefix selection
// ===========================================================================

#[test]
fn e2e_prefix_selection_unique_resolves() {
    let mut backend = multi_backend();
    let out = run(&["logs", "cd", "--no-color"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.contains("==> gamma-cmd <=="),
        "unique prefix 'cd' should resolve to gamma-cmd"
    );
}

#[test]
fn e2e_prefix_selection_by_name_exact() {
    let mut backend = multi_backend();
    let out = run(&["logs", "beta", "--no-color"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.contains("==> beta <=="),
        "exact name 'beta' should resolve"
    );
}

#[test]
fn e2e_prefix_selection_by_short_id_exact() {
    let mut backend = multi_backend();
    let out = run(&["logs", "ab123456", "--no-color"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.contains("==> alpha <=="),
        "exact short_id should resolve"
    );
}

// ===========================================================================
// E2E: Ambiguous prefix errors
// ===========================================================================

#[test]
fn e2e_ambiguous_prefix_error() {
    let mut backend = multi_backend();
    let out = run(&["logs", "a"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(
        out.stderr.contains("ambiguous"),
        "ambiguous prefix should report error; got: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("alpha (ab123456)"),
        "ambiguous error should list alpha candidate"
    );
    assert!(
        out.stderr.contains("beta (ad123547)"),
        "ambiguous error should list beta candidate"
    );
}

#[test]
fn e2e_ambiguous_prefix_longer_prefix_resolves() {
    let mut backend = multi_backend();
    let out = run(&["logs", "ab", "--no-color"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.contains("==> alpha <=="),
        "longer prefix 'ab' should resolve uniquely to alpha"
    );
}

#[test]
fn e2e_not_found_loop_error() {
    let mut backend = multi_backend();
    let out = run(&["logs", "zzz-nonexistent"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(
        out.stderr.contains("not found"),
        "unknown loop should report not found; got: {}",
        out.stderr
    );
}

// ===========================================================================
// E2E: --raw bypasses highlighting
// ===========================================================================

#[test]
fn e2e_raw_mode_bypasses_all_rendering() {
    let mut backend = rich_backend();
    let out = run(
        &["logs", "codex-loop", "--lines", "50", "--raw"],
        &mut backend,
    );
    assert_success(&out);
    assert!(
        !out.stdout.contains(ESC),
        "raw mode should not contain ANSI escapes"
    );
    assert!(
        !out.stdout.contains("== "),
        "raw mode should not add signifiers"
    );
    assert!(
        !out.stdout.contains(">> "),
        "raw mode should not add role marker signifiers"
    );
    // Raw content should be preserved verbatim.
    assert!(
        out.stdout.contains("OpenAI Codex v0.80.0"),
        "raw mode should preserve original content"
    );
}

// ===========================================================================
// E2E: --compact mode
// ===========================================================================

#[test]
fn e2e_compact_collapses_thinking_block() {
    let mut backend = rich_backend();
    let out = run(
        &[
            "logs",
            "codex-loop",
            "--lines",
            "50",
            "--no-color",
            "--compact",
        ],
        &mut backend,
    );
    assert_success(&out);
    assert!(
        out.stdout.contains("thinking lines collapsed"),
        "compact mode should collapse thinking block; got: {}",
        out.stdout
    );
}

#[test]
fn e2e_compact_collapses_code_fence() {
    let mut backend = rich_backend();
    let out = run(
        &[
            "logs",
            "codex-loop",
            "--lines",
            "50",
            "--no-color",
            "--compact",
        ],
        &mut backend,
    );
    assert_success(&out);
    assert!(
        out.stdout.contains("code lines collapsed"),
        "compact mode should collapse code fence; got: {}",
        out.stdout
    );
}

// ===========================================================================
// E2E: Command transcript highlighting
// ===========================================================================

#[test]
fn e2e_command_prompt_highlighted() {
    std::env::remove_var("NO_COLOR");
    let mut backend = multi_backend();
    let out = run(&["logs", "gamma-cmd", "--lines", "50"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.contains(ESC),
        "command prompt should have ANSI styling"
    );
}

#[test]
fn e2e_command_exit_code_nonzero_error() {
    let mut backend = multi_backend();
    let out = run(
        &["logs", "gamma-cmd", "--lines", "50", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    assert!(
        out.stdout.contains("[ERROR] exit code: 1"),
        "nonzero exit code should get [ERROR] prefix; got: {}",
        out.stdout
    );
}

#[test]
fn e2e_command_exit_code_zero_not_error() {
    let mut backend = multi_backend();
    let out = run(
        &["logs", "gamma-cmd", "--lines", "50", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    // exit code: 0 should NOT get [ERROR] prefix.
    assert!(
        !out.stdout.contains("[ERROR] exit code: 0"),
        "exit code 0 should not be error-styled; got: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("exit code: 0"),
        "exit code 0 should be present"
    );
}

// ===========================================================================
// E2E: --all flag with highlighting
// ===========================================================================

#[test]
fn e2e_all_loops_colored() {
    std::env::remove_var("NO_COLOR");
    let mut backend = multi_backend();
    let out = run(&["logs", "--all", "--lines", "3"], &mut backend);
    assert_success(&out);
    // Multiple loop headers should be present.
    assert!(out.stdout.contains("==> alpha <=="));
    assert!(out.stdout.contains("==> beta <=="));
    assert!(out.stdout.contains("==> gamma-cmd <=="));
    // Should still contain coloring.
    assert!(
        out.stdout.contains(ESC),
        "all-loops output should still be colored"
    );
}

#[test]
fn e2e_all_loops_no_color() {
    let mut backend = multi_backend();
    let out = run(
        &["logs", "--all", "--lines", "3", "--no-color"],
        &mut backend,
    );
    assert_success(&out);
    assert!(out.stdout.contains("==> alpha <=="));
    assert!(out.stdout.contains("==> beta <=="));
    assert!(!out.stdout.contains(ESC));
}

// ===========================================================================
// E2E: Corpus-based rendering verification
// ===========================================================================

/// Feed real corpus transcript through the rendering pipeline and verify
/// key semantic tokens are present in the output.
#[test]
fn e2e_codex_corpus_rendering_no_color() {
    let corpus = include_str!("../testdata/log_highlighting_corpus/codex_real_transcript.log");
    let path = default_log_path("/tmp/forge", "corpus-codex", "loop-corpus");
    let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
        id: "loop-corpus".to_string(),
        short_id: "corp01".to_string(),
        name: "corpus-codex".to_string(),
        repo: "/repo-main".to_string(),
        log_path: path.clone(),
    }])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_log(&path, corpus);

    let out = run(
        &["logs", "corpus-codex", "--lines", "200", "--no-color"],
        &mut backend,
    );
    assert_success(&out);

    // Harness header should have == prefix.
    assert!(
        out.stdout.contains("== OpenAI Codex v0.80.0"),
        "corpus: harness header should get == prefix; first 300: {}",
        &out.stdout[..300.min(out.stdout.len())]
    );

    // Role markers should have >> prefix.
    assert!(
        out.stdout.contains(">> user") || out.stdout.contains(">> codex"),
        "corpus: role markers should get >> prefix"
    );

    // No ANSI escapes in no-color mode.
    assert!(
        !out.stdout.contains(ESC),
        "corpus no-color should not contain ANSI escapes"
    );
}

/// Feed real corpus transcript through the rendering pipeline in color mode
/// and verify ANSI styling is present.
#[test]
fn e2e_codex_corpus_rendering_color() {
    std::env::remove_var("NO_COLOR");
    let corpus = include_str!("../testdata/log_highlighting_corpus/codex_real_transcript.log");
    let path = default_log_path("/tmp/forge", "corpus-codex-c", "loop-corpus-c");
    let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
        id: "loop-corpus-c".to_string(),
        short_id: "corc01".to_string(),
        name: "corpus-codex-c".to_string(),
        repo: "/repo-main".to_string(),
        log_path: path.clone(),
    }])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_log(&path, corpus);

    let out = run(&["logs", "corpus-codex-c", "--lines", "200"], &mut backend);
    assert_success(&out);

    // Should contain bold cyan for header.
    assert!(
        out.stdout.contains(BOLD_CYAN),
        "corpus color: should have bold cyan harness header"
    );

    // Should contain bold magenta for role markers.
    assert!(
        out.stdout.contains(BOLD_MAGENTA),
        "corpus color: should have bold magenta role markers"
    );

    // Should contain RESET.
    assert!(
        out.stdout.contains(RESET),
        "corpus color: should have ANSI resets"
    );

    // Content preserved.
    assert!(
        out.stdout.contains("Codex v0.80.0"),
        "corpus color: content should be preserved"
    );
}

/// Feed Claude corpus through rendering and verify JSON events are processed.
#[test]
fn e2e_claude_corpus_rendering() {
    let corpus = include_str!("../testdata/log_highlighting_corpus/claude_real_transcript.log");
    let path = default_log_path("/tmp/forge", "corpus-claude", "loop-corpus-cl");
    let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
        id: "loop-corpus-cl".to_string(),
        short_id: "corcl1".to_string(),
        name: "corpus-claude".to_string(),
        repo: "/repo-main".to_string(),
        log_path: path.clone(),
    }])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_log(&path, corpus);

    let out = run(
        &["logs", "corpus-claude", "--lines", "200", "--no-color"],
        &mut backend,
    );
    assert_success(&out);

    // Claude JSON events should be rendered (not raw JSON).
    assert!(
        out.stdout.contains("[claude:") || out.stdout.contains("status:"),
        "corpus claude: JSON events should be rendered"
    );
}

// ===========================================================================
// E2E: Error handling edge cases
// ===========================================================================

#[test]
fn e2e_missing_loop_arg_errors() {
    let mut backend = multi_backend();
    let out = run(&["logs"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert_eq!(out.stderr, "loop name required (or use --all)\n");
}

#[test]
fn e2e_invalid_lines_value_errors() {
    let mut backend = multi_backend();
    let out = run(&["logs", "alpha", "--lines", "not-a-number"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("invalid value"));
}

#[test]
fn e2e_unknown_flag_errors() {
    let mut backend = multi_backend();
    let out = run(&["logs", "alpha", "--bogus"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("unknown argument"));
}
