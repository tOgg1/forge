# TUI-204 failure jump and root-cause focus mode

Task: `forge-110`  
Status: delivered

## Scope

- Jump to first failure in log streams.
- Chain failure -> root cause -> linked command context.
- Produce highlight metadata for cause-focused rendering.

## Implementation

- New module: `crates/forge-tui/src/failure_focus.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core API:

- `jump_to_first_failure(...)`
- `jump_to_root_cause(...)`
- `build_failure_focus(...)`

Core model:

- `FailureFocus`: failure/root-cause indices + linked command context
- `HighlightedLine`: semantic highlight tags
- `CauseLink`: ordered breadcrumb links for cause navigation
- `HighlightRole`: `Failure | RootCause | CommandContext | CauseContext`

## Behavior

- Failure detection:
  - picks first failure-like line
  - ignores known success-only phrases (`0 failed`, `all tests passed`, ...)
- Root cause extraction:
  - scans backward from failure for explicit markers (`caused by`, `failed to`, ...)
  - falls back to earliest failure in local failure block
- Command context:
  - links nearest prior command-like line (`$ ...`, `running: ...`, ...)
- Cause chain navigation:
  - exposes sorted unique chain lines
  - supports next/previous jump semantics

## Regression tests

Added tests in `crates/forge-tui/src/failure_focus.rs` for:

- first-failure detection + success-shape filtering
- explicit root-cause marker detection
- command-context linkage
- highlight role assignment
- next/previous chain jump behavior
- fallback behavior when explicit root-cause marker is absent
- bounds handling for failure override and empty input

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
