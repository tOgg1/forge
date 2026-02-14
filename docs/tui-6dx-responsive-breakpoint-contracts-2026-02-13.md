# TUI-6DX responsive breakpoint contracts

## Scope
- Added explicit Xs/Sm/Md/Lg breakpoint contracts for multi-pane layout fallback.
- Goal: deterministic tiered fallback under constrained terminals.

## Changes
- `crates/forge-tui/src/layouts.rs`
  - Added `BreakpointTier` (`Xs`, `Sm`, `Md`, `Lg`).
  - Added `BreakpointContract` with row/col caps + min-cell requirements.
  - Added `classify_breakpoint(width, height)`.
  - Added `breakpoint_contract(width, height, min_cell_width, min_cell_height)`.
  - Added `fit_pane_layout_for_breakpoint(...)` wrapper that applies tier contract before fitting.
  - Added regression tests for tier mapping and deterministic fallback outcomes.
- `crates/forge-tui/src/app.rs`
  - `effective_multi_layout()` now uses `fit_pane_layout_for_breakpoint(...)`.
- `crates/forge-tui/src/multi_logs.rs`
  - Multi-log renderer now uses `fit_pane_layout_for_breakpoint(...)`.
- `crates/forge-tui/src/layout_presets.rs`
  - Preset application now uses `fit_pane_layout_for_breakpoint(...)`.
- Layout snapshots
  - Refreshed `crates/forge-tui/tests/golden/layout/multi_logs_80x24.txt` to match new deterministic Xs behavior.

## Validation
- `cargo test -p forge-tui layouts::tests:: -- --nocapture`
- `cargo test -p forge-tui --test layout_snapshot_test -- --nocapture`
- `cargo build -p forge-tui`
