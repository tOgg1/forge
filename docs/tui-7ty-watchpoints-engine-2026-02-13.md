# TUI-7TY watchpoints engine core

## Scope
- Add a reusable watchpoint evaluation core for conditional metric alerts.

## Changes
- Added `crates/forge-tui/src/watchpoints.rs`:
  - typed value model (`WatchValue`)
  - comparators (`WatchComparator`)
  - condition mode (`All`/`Any`)
  - watchpoint definition and runtime state
  - evaluator: `evaluate_watchpoints(...)` with cooldown and active-state tracking
  - persistence helpers: `persist_watchpoints(...)` / `restore_watchpoints(...)`
  - deterministic trigger payload model (`WatchpointTrigger`)
- Exported module in `crates/forge-tui/src/lib.rs`.

## Validation
- `cargo test -p forge-tui watchpoints::tests:: -- --nocapture`
- `cargo build -p forge-tui`
