# TUI-702 data polling pipeline with backpressure and jitter control

Task: `forge-2er`
Status: delivered

## Scope

- Stabilize TUI refresh cadence under load.
- Add bounded polling buffer to prevent unbounded snapshot growth.
- Add controlled deterministic jitter to avoid thundering-herd polling alignment.

## Model

- New module: `crates/forge-tui/src/polling_pipeline.rs`
  - `PollingConfig` for cadence, jitter, and backpressure limits
  - `PollingQueue<T>` bounded queue with oldest-drop policy
  - `PollScheduler` deterministic jittered interval scheduler
  - `deterministic_jitter_ms` helper for reproducible jitter values

## Derivation rules

- Queue behavior:
  - enforce `max_pending_snapshots` capacity
  - on overflow, drop oldest snapshot and retain newest
  - render path drains queue to latest snapshot (backlog collapse)
- Scheduling behavior:
  - `interval = base_interval + jitter + backpressure_penalty`
  - jitter bounded by `max_jitter_ms` and deterministic per pipeline key + tick
  - backpressure penalty scales with backlog and caps at `max_backpressure_ms`

## Integration

- `crates/forge-tui/src/bin/forge-tui.rs` now uses `PollScheduler` + `PollingQueue` in interactive mode:
  - polls on jittered cadence
  - queues snapshots with bounded buffering
  - renders latest snapshot only to keep UI responsive under transient load
  - exposes runtime poll/queue status line (`queue:max`, `dropped`)
- Module exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
