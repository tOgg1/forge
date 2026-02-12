# TUI-305 stale in-progress detector and takeover suggestions

Task: `forge-p67`  
Status: delivered

## Scope

- Detect stale `in_progress` tasks and active loops.
- Emit recommended recovery actions.
- Provide explicit takeover guidance when safe.
- Include false-positive mitigation controls.

## Implementation

- New module: `crates/forge-tui/src/stale_takeover.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Detection model

- `build_stale_takeover_report(...)` evaluates task + loop samples against policy:
  - `task_stale_after_secs`
  - `loop_stale_after_secs`
  - `required_observations`
  - `recent_activity_grace_secs`
  - `min_loop_queue_depth`
- Output report includes:
  - `alerts` (watch/takeover severity)
  - `suppressed` candidates with mitigation reason

## False-positive mitigation controls

- Require repeated stale observations before alerting.
- Suppress when recent activity is inside grace window.
- Suppress loop alerts when queue depth is below minimum and no active tasks.
- Suppress non-active statuses/states.

## Suggested recovery actions

- Stale unblocked tasks:
  - takeover claim command hint
  - safeguards for ownership confirmation + thread audit
- Blocked stale tasks:
  - watch-only guidance to resolve blockers first
- Stale active loops:
  - recovery workflow hint (`forge ps` + refocus message template)

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
