# TUI-921 scheduled actions timer queue

Task: `forge-h54`  
Status: delivered

## Scope

- Scheduled action queue for delayed operator intents:
  - snooze inbox thread
  - delayed loop restart
  - time-based auto-acknowledge thread
- Deterministic timer queue API with due-drain, cancellation, and compact status summary.
- Status bar timer visibility wired to snoozed-notification timers.

## Implementation

- New module: `crates/forge-tui/src/scheduled_actions.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`
- Status bar timer summary in `crates/forge-tui/src/app.rs`:
  - appends `timers:N next:Mt` when pending timers exist

### Core API

- `ScheduledActionSpec`: typed scheduled intents (`snooze`, `delayed restart`, `auto-ack`).
- `ScheduledActionQueue`:
  - `schedule(...)`
  - `cancel(...)`
  - `advance_clock(...)`
  - `next_due_in_ticks()`
  - `compact_timer_summary()`

## Regression Coverage

- `scheduled_actions` module tests:
  - stable ids / due ordering
  - due-drain behavior
  - cancel behavior
  - summary formatting
  - normalization/default-reason behavior
  - all action kinds present
  - metric check: pending timer visible at `<=5` ticks
- `app.rs` tests:
  - snoozed timer status display progression (`3t -> 1t -> resurfaced`)
  - status line suffix includes timer summary when status text already present

## Validation

- `cargo fmt --all`
- `cargo test -p forge-tui scheduled_actions:: -- --nocapture`
- `cargo test -p forge-tui notification_center_snooze_hides_until_clock_advances -- --nocapture`
- `cargo test -p forge-tui status_display_appends_timer_summary_when_status_present -- --nocapture`
- `cargo build -p forge-tui`
