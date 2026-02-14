# TUI typed-reason destructive confirms (forge-gwc)

Date: 2026-02-13
Task: forge-gwc

## What changed
- Confirm modal now requires typed reason text for high-risk actions:
  - `Kill`
  - `Delete` when target loop is not `stopped` (force-delete path)
- Added confirm metadata fields:
  - `reason`, `reason_required`
  - `force_delete` (explicit flag; no prompt-string parsing)
- Enforced minimum reason length: `12` chars before submit.
- Added confirm-mode input handling for reason editing:
  - character input
  - `Backspace`
  - `Ctrl+U` clear
- Kept safe action rail default on `Cancel`.
- Updated confirm modal render/help copy for typed-reason workflow.

## Regression coverage
- Added/updated tests in `crates/forge-tui/src/app.rs`:
  - `confirm_kill_requires_typed_reason_before_submit`
  - `confirm_kill_submit_after_reason_input`
  - `force_delete_requires_typed_reason_before_submit`
  - Extended delete confirm tests to assert `reason_required` toggles correctly.
- Updated integrated smoke flows in `crates/forge-tui/tests/interactive_smoke_test.rs`:
  - `confirm_kill_then_accept`
  - `confirm_delete_then_accept`

## Validation
- `cargo fmt --all`
- `cargo test -p forge-tui confirm_kill_ -- --nocapture`
- `cargo test -p forge-tui force_delete_requires_typed_reason_before_submit -- --nocapture`
- `cargo test -p forge-tui delete_running_loop_shows_force -- --nocapture`
- `cargo test -p forge-tui delete_stopped_loop_normal -- --nocapture`
- `cargo build -p forge-tui`
