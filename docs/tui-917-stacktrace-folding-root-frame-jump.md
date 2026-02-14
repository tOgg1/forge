# TUI-917 stacktrace folding + root-frame jump

Task: `forge-zf1`  
Status: delivered (code + regression tests; workspace build currently blocked by unrelated `forge-cli` errors)

## Scope

- Fold stacktrace-heavy log blocks to reduce noise.
- Jump to probable application root frame for faster failure triage.

## Implementation

- `crates/forge-tui/src/log_pipeline.rs`
  - Added `SectionKind::StackTrace`.
  - Added stacktrace-line detection in section classifier.
  - Made stacktrace blocks foldable (`fold_all`, per-block fold, summary rendering).
- `crates/forge-tui/src/failure_focus.rs`
  - Added `jump_to_probable_root_frame(...)`.
  - Added `FailureFocus.root_frame_line`.
  - Added root-frame link/highlight role (`HighlightRole::RootFrame`).
  - Added heuristics to prefer application frames over library/runtime frames.
- `crates/forge-tui/tests/stacktrace_focus_test.rs`
  - Regression tests for stacktrace fold behavior.
  - Regression tests for root-frame jump selection + metadata.

## Validation

Attempted:

- `cargo test -p forge-tui --test stacktrace_focus_test -- --nocapture`
- `cargo build -p forge-tui`

Blocked by unrelated existing compile errors in `crates/forge-cli/src/workflow_run_persistence.rs`
(`E0252`: duplicate imports `Write`, `PathBuf`, `Utc`).
