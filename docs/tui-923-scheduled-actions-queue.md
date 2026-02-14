# TUI-923 scheduled actions queue

Task: `forge-h54`  
Status: delivered

## Scope

- Delayed actions for:
  - thread snooze + auto-resurface
  - delayed loop restart
  - time-based thread auto-ack
- Visible timer queue status string for status bar embedding.
- Status bar timer suffix for snoozed notifications: `timers:N next:Mt`.

## Implementation

- New module: `crates/forge-tui/src/scheduled_actions.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core API:

- `ScheduledActionQueue::schedule_at(...)`
- `ScheduledActionQueue::schedule_after(...)`
- `ScheduledActionQueue::cancel(...)`
- `ScheduledActionQueue::due_actions(...)`
- `ScheduledActionQueue::pop_due_actions(...)`
- `ScheduledActionQueue::status_line(now, max_items)`

Core model:

- `ScheduledActionKind`
- `ScheduledAction`
- `DueScheduledAction`
- `ScheduledActionQueue`

Behavior:

- deterministic due-order sorting (`due_at`, then `schedule_id`)
- target and future-time validation
- due-action peek vs pop semantics
- compact countdown status string (`timers=n kind:target@+Ns`)

## Regression tests

Added in `crates/forge-tui/src/scheduled_actions.rs`:

- due pop order correctness
- status-line countdown rendering
- cancel by id behavior
- validation for empty target and zero delay
- due peek non-destructive semantics

Updated in `crates/forge-tui/src/app.rs`:

- `notification_center_snooze_hides_until_clock_advances`
- `status_display_appends_timer_summary_when_status_present`

## Validation

- `cargo test -p forge-tui scheduled_actions -- --nocapture`
- `cargo test -p forge-tui notification_center_snooze_hides_until_clock_advances -- --nocapture`
- `cargo test -p forge-tui status_display_appends_timer_summary_when_status_present -- --nocapture`
- `cargo build -p forge-tui`
