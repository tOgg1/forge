# TUI-927 bulk action mixed-target dry-run

Task: `forge-m38`  
Status: delivered

## Scope

- Extend bulk planner to true multi-select mode across loops and threads.
- Keep dry-run previews explicit about ignored/mismatched targets before apply.

## Implementation

- Updated `crates/forge-tui/src/bulk_action_planner.rs`.
- Added thread-aware models:
  - `BulkThreadRecord`
  - `BulkPlannerTarget::{Loop, Thread}`
- Added thread actions:
  - `BulkPlannerAction::AckThread`
  - `BulkPlannerAction::ReplyThread { body }`
- Added mixed-target planner:
  - `plan_bulk_action_mixed(action, selected, preview_limit)`
  - Loop actions (`stop/scale/msg/inject`) now:
    - plan loop queue entries as before
    - emit blocked queue items + warnings for thread targets
  - Thread actions (`ack/reply`) now:
    - plan per-thread command queue
    - enforce payload requirements for reply
    - no-op warning for ack on threads without pending ack
    - emit blocked queue items + warnings for loop targets
- Added command + rollback helpers for thread actions.

## Validation

- `cargo test -p forge-tui bulk_action_planner::tests::`
- `cargo build -p forge-tui`
