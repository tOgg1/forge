# TUI-607 alert rail for failures, stuck loops, and queue growth

Task: `forge-67x`  
Status: delivered

## Scope

- Add sticky alert-strip planning for loop failures, stuck loops, and queue growth.
- Prioritize alerts deterministically for a compact status rail.
- Attach quick-jump loop targets so operators can pivot to affected loops fast.

## Implementation

- New module: `crates/forge-tui/src/alert_rail.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core model:

- `LoopAlertSample`: normalized per-loop inputs (`failures`, `last progress`, `queue depth` delta)
- `AlertRailPolicy`: thresholds + sticky hold window + max rail size
- `AlertRailAlert`: typed alert row with severity, reason summary, and jump hint
- `AlertRailState`: sticky alert buffer across refresh ticks
- `AlertStripPlan`: compact strip headline + quick-jump target list

Core API:

- `build_alert_rail_state(...)` detects alert conditions and keeps recovered alerts sticky for a bounded recovery window.
- `plan_alert_strip(...)` builds sticky strip metadata (`headline`, `count`, quick-jump targets).
- `quick_jump_loop_id(...)` resolves numeric jump slots to loop ids.

Detection rules:

- `failure spike`: `recent_failures >= failure_threshold`
- `stuck loop`: `now - last_progress >= stuck_after_secs`
- `queue growth`: depth increase crosses both delta and percent thresholds at minimum depth

## Regression tests

Added tests in `crates/forge-tui/src/alert_rail.rs` for:

- multi-condition detection (failure/stuck/growth)
- sticky recovery hold and bounded expiration
- stable unique quick-jump target mapping
- queue-growth threshold gating
- empty-strip fallback headline behavior

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
