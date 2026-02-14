# TUI Claim Timeline Rail Seam (2026-02-13)

Task: `forge-29p`  
Scope: inbox layout seam for claim timeline rail.

## Verification result

Seam exists in `crates/forge-tui/src/app.rs`:

- Inbox pane reserves bottom rail rows when claim events exist.
- Rail renders as `Claim Timeline (latest)` with conflict highlighting.
- Conflict selection/status shortcuts wired in inbox mode.

## Validation

- `cargo test -p forge-tui --lib inbox_render_uses_cli_mail_ids_and_threads -- --nocapture`
- `cargo test -p forge-tui --lib inbox_claim_conflict_shortcuts_show_status -- --nocapture`
- `cargo test -p forge-tui --lib inbox_handoff_snapshot_uses_claim_fallback_when_task_not_in_thread -- --nocapture`
- `cargo build -p forge-tui`
