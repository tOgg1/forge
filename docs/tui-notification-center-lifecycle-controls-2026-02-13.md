# TUI notification center lifecycle controls (forge-7pg)

Date: 2026-02-13
Task: forge-7pg

## What changed
- Extended in-app notification queue events with lifecycle fields:
  - `acknowledged`
  - `escalated`
  - `snoozed_until_sequence`
- Added lifecycle control APIs on `App`:
  - `notification_center_ack_latest()`
  - `notification_center_escalate_latest()`
  - `notification_center_snooze_latest(ticks)`
  - `advance_notification_clock(ticks)`
  - `notification_center_entries()`
- Added notification center projection struct:
  - `NotificationCenterEntry { kind, text, acknowledged, escalated, snoozed }`
- Updated status fallback logic:
  - status bar now falls back to latest *visible* queue event
  - hidden events: acknowledged or currently snoozed
  - queued suffix now counts visible queue backlog

## Regression coverage
- Added tests in `crates/forge-tui/src/app.rs`:
  - `notification_center_ack_hides_latest_from_status_fallback`
  - `notification_center_snooze_hides_until_clock_advances`
  - `notification_center_entries_include_escalation_and_snooze_flags`
- Existing status display tests still pass with visibility-aware counting.

## Validation
- `cargo test -p forge-tui notification_center`
- `cargo test -p forge-tui status_display_`
- `cargo build -p forge-tui`
