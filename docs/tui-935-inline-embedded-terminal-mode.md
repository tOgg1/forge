# TUI-935: Inline Embedded Terminal Mode

Task: `forge-bch`

## Scope
- Provide compact inline Forge status mode for shell-first operators.
- Support 1-line and 3-line inline densities.
- Allow one-key toggle between inline mode and full TUI.

## Implementation
- Added `crates/forge-tui/src/inline_terminal_mode.rs`.
- Core model:
  - `TerminalDisplayMode` (`InlineSingle`, `InlineTriple`, `FullTui`)
  - `InlineStatusSnapshot`
  - `InlineTerminalState`
  - `ToggleOutcome`
- Core behavior:
  - `toggle_inline_full_mode(...)` (keypress_count = 1)
  - `cycle_inline_density(...)`
  - `render_inline_lines(...)`
- Rendering:
  - single-line status bar with loop/runs/queue/fmail/focus metrics
  - triple-line mode with split overview/focus/status rows
  - full-mode hint line for toggle fallback
- Exported via `crates/forge-tui/src/lib.rs`.

## Regression Tests
- `inline_terminal_mode::tests::toggle_inline_full_mode_is_one_keypress`
- `inline_terminal_mode::tests::cycle_density_switches_single_and_triple_inline_modes`
- `inline_terminal_mode::tests::cycle_density_noops_in_full_mode`
- `inline_terminal_mode::tests::render_inline_single_line_includes_core_metrics`
- `inline_terminal_mode::tests::render_inline_triple_line_shows_focus_and_status`

## Validation
- `cargo fmt --package forge-tui`
- `cargo test -p forge-tui inline_terminal_mode::tests:: -- --nocapture`
- `cargo build -p forge-tui`
