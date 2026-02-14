# TUI-934: Tmux-Aware Integration

Task: `forge-f5y`

## Scope
- Detect tmux runtime context.
- Provide pane-native action plans:
  - send logs to adjacent pane
  - open run details in a new split
  - share clipboard via tmux buffer
- Graceful fallback when not running inside tmux.
- Keep run-detail split flow within 2 keypresses.

## Implementation
- Added `crates/forge-tui/src/tmux_integration.rs`.
- Core model:
  - `TmuxContext`
  - `PaneDirection`, `SplitOrientation`
  - `TmuxCommandPlan`
- Core functions:
  - `detect_tmux_context(...)`
  - `build_send_log_to_adjacent_pane_plan(...)`
  - `build_open_run_details_split_plan(...)`
  - `build_share_clipboard_via_tmux_buffer_plan(...)`
  - `render_tmux_plan_lines(...)`
- Included quoting + width-fit helpers for stable command rendering.
- Exported via `crates/forge-tui/src/lib.rs`.

## Regression Tests
- `tmux_integration::tests::detect_tmux_context_from_env`
- `tmux_integration::tests::detect_tmux_context_handles_absent_env`
- `tmux_integration::tests::send_log_plan_uses_adjacent_target_and_is_one_keypress`
- `tmux_integration::tests::open_run_details_plan_meets_two_keypress_target`
- `tmux_integration::tests::clipboard_share_gracefully_degrades_outside_tmux`

## Validation
- `cargo fmt --package forge-tui`
- `cargo test -p forge-tui tmux_integration::tests:: -- --nocapture`
- `cargo build -p forge-tui`
