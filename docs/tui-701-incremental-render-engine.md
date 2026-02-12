# TUI-701 Incremental Render Engine (forge-qxw)

Date: 2026-02-12
Scope: reduce terminal writes by diffing current frame lines against previous frame and repainting changed rows only.

## Implementation

Core path: `crates/forge-tui/src/bin/forge-tui.rs`

- `IncrementalRenderEngine` stores `previous_lines` and applies a row-level diff.
- `plan_render_diff(previous, next)` computes:
  - `changed_rows` (modified or appended rows)
  - `clear_start_row` / `clear_end_row` for removed tail rows
- `repaint(...)` emits ANSI row rewrites only for changed rows, clears stale rows when frame shrinks, then parks cursor below rendered content.

## Regression tests

Added focused tests in `crates/forge-tui/src/bin/forge-tui.rs`:

- diff plan for changed+appended rows
- diff plan for shrinking frame clear range
- noop repaint when frame is unchanged
- repaint output includes only changed row writes and tail clear commands

This hardens incremental repaint behavior for high-frequency refresh loops.
