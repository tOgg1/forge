# TUI Evidence Hotkeys (forge-67w)

Date: 2026-02-13
Task: `forge-67w`

## What shipped

Added keyboard evidence jumps with sticky return in `forge-tui` main mode:

- `Ctrl+E`: jump to latest `ERROR` evidence
- `Ctrl+W`: jump to latest `WARN` evidence
- `Ctrl+A`: jump to latest pending `ACK` evidence (Inbox)
- `Ctrl+B`: return to pre-jump context

Behavior:

- Evidence jump stores one sticky return point (tab + loop/run/thread selection + scroll/focus state).
- If no evidence exists, status line reports this without moving focus.
- `Ctrl+B` restores the exact prior context and clears the sticky point.

Resolution order for `ERROR`/`WARN`:

1. Current active line source (`Runs` output or selected `Logs`) newest-match first.
2. Run history (status/output evidence) newest-match first.
3. Loop list (`state`/`last_error`) with latest `last_run_at` preference.

`ACK` jump targets the newest Inbox thread with pending acknowledgements.

## UI discoverability updates

- Footer hints: include evidence jump (`ctrl+e`) and return (`ctrl+b`) hints.
- Help panel: explicit global hotkey docs for `Ctrl+E/W/A` and `Ctrl+B`.
- Keymap registry: added dedicated commands for all four hotkeys.

## Validation

Executed:

- `cargo build -p forge-tui`
- `cargo test -p forge-tui --lib ctrl_e_jumps_to_latest_error_line_and_ctrl_b_restores_scroll`
- `cargo test -p forge-tui --lib ctrl_w_jumps_to_warning_run_and_ctrl_b_restores_previous_tab`
- `cargo test -p forge-tui --lib ctrl_a_jumps_to_latest_ack_thread_and_ctrl_b_restores_tab`
- `cargo test -p forge-tui --lib ctrl_b_without_prior_jump_reports_missing_return_point`
- `cargo test -p forge-tui --lib help_lists_evidence_hotkeys`
- `cargo test -p forge-tui --lib resolves_with_scope_precedence_snapshot`
