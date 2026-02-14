# TUI-928: State Persistence Across Restarts

Task: `forge-znd`

## Scope
- Extend persisted session snapshot to include log scroll position.
- Bridge `App` state to/from `session_restore::SessionContext`.
- Restore tab/layout/filter/selection/focus/pins safely with fallback notices.

## Implementation
- `crates/forge-tui/src/session_restore.rs`
  - Added `log_scroll` to `SessionContext` and `PersistedSessionSnapshot`.
  - Snapshot capture now persists `log_scroll`.
  - Restore flow now returns `log_scroll`.
  - Delta digest includes `log scroll changed: <old> -> <new>`.
- `crates/forge-tui/src/crash_safe_state.rs`
  - Added parse/serialize support for `log_scroll` in crash-safe snapshot JSON.
  - Added round-trip assertion for recovered `log_scroll`.
- `crates/forge-tui/src/app.rs`
  - Added `session_restore_context()` to export app tab/layout/filter/selection/focus/pins/log scroll.
  - Added `restore_from_session_context()` to safely apply stored context with notices for unavailable values.
  - Restore now clamps scroll to `MAX_LOG_BACKFILL` and updates follow mode accordingly.
  - Restore now fully replaces pinned set (including restoring empty).

## Regression Tests
- `app::tests::session_restore_round_trip_restores_tab_layout_selection_and_scroll`
- `app::tests::restore_from_session_context_reports_unavailable_values_and_clamps_scroll`
- `session_restore::tests::*` (7 tests)
- `crash_safe_state::tests::persist_context_snapshot_round_trip`

## Validation
- `cargo fmt --package forge-tui`
- `cargo test -p forge-tui session_restore_round_trip_restores_tab_layout_selection_and_scroll -- --nocapture`
- `cargo test -p forge-tui restore_from_session_context_reports_unavailable_values_and_clamps_scroll -- --nocapture`
- `cargo test -p forge-tui session_restore::tests:: -- --nocapture`
- `cargo test -p forge-tui crash_safe_state::tests::persist_context_snapshot_round_trip -- --nocapture`
- `cargo build -p forge-tui`
