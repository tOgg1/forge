# TUI-806 configurable status strip and widget slots

Task: `forge-67r`  
Status: delivered

## Scope

- Configurable top + bottom status strips.
- Pluggable widget registry.
- Persisted widget ordering + enable state.

## Implementation

- New module: `crates/forge-tui/src/status_strip.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Model

- Widget definitions:
  - `StatusWidgetDefinition`
  - `StatusWidgetRegistry` (builtins + plugin registration)
- Persisted state:
  - `StatusStripStore`
  - `StatusStripPlacement`
  - `STATUS_STRIP_SCHEMA_VERSION=2`
- Runtime planning/render:
  - `build_status_strip_plan(...)`
  - `render_status_strip_line(...)`

## Persistence behavior

- Supports schema migration from legacy v1 (`top`/`bottom`/`disabled`) to v2 placements.
- Unknown/duplicate widgets are ignored with warnings.
- Missing widgets are backfilled from registry defaults.
- Orders normalized per strip (top/bottom).

## Operator actions

- `move_widget_slot(...)`: move widget across strip + slot index.
- `set_widget_enabled(...)`: toggle widget visibility.

## Validation

- Unit tests in `crates/forge-tui/src/status_strip.rs` cover:
  - defaults and slot planning
  - plugin registration
  - v1/v2 restore and migration warnings
  - move/toggle actions
  - deterministic render + truncation
