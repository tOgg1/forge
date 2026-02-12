# TUI-506 wind-down workflow and final state reconciliation

Task: `forge-5mw`
Status: delivered

## What shipped

- Added wind-down workflow module: `crates/forge-tui/src/swarm_wind_down.rs`.
- Added graceful-stop sequence evaluator with staged outcomes:
  - `Pending`
  - `Completed`
  - `Blocked`
- Added stale-check stage with explicit threshold validation (`stale_minutes` vs threshold).
- Added ledger-sync stage for final ledger reconciliation before closure.
- Added final closure reconciliation with summary generation per loop/swarm, including:
  - runtime state
  - stale check values
  - ledger sync status
  - outstanding task count
- Added deterministic report ordering for stable TUI rendering (`swarm_id`, then `loop_id`).
- Added regression tests for:
  - clean closable wind-down
  - stale + unsynced + outstanding blockers
  - active loop pending graceful stop
  - deterministic ordering
- Exported module from crate root: `crates/forge-tui/src/lib.rs`.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
