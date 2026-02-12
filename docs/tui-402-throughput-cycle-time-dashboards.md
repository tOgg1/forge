# TUI-402 throughput and cycle-time dashboards

Task: `forge-350`
Status: delivered

## Scope

- Add analytics dashboard model for:
  - run throughput chart
  - task cycle-time table
  - queue-aging table
  - completion-velocity chart
- Provide deterministic summaries for operator planning and bottleneck review.

## Model

- Inputs:
  - `ThroughputBucketSample`
  - `TaskLifecycleSample`
  - `DashboardInput`
- Outputs:
  - `AnalyticsDashboardView`
  - `ChartPoint`
  - `CycleTimeRow`
  - `QueueAgingRow`
  - `DashboardSummary` (throughput/cycle/aging/velocity)

## Derivation rules

- Throughput chart:
  - one point per bucket with completed run count + started/completed/failed detail
  - ASCII bar rendering with deterministic width
- Cycle-time table:
  - include tasks with valid `started_at` + `completed_at`
  - sort by longest cycle time first
  - compute `p50`, `p90`, `max`
- Queue-aging table:
  - include incomplete tasks only
  - age by `now - queue_entered_at`
  - stale flag using configurable threshold (`queue_stale_after_secs`, default `3600`)
- Completion velocity:
  - hourly window buckets (`velocity_window_hours`, default `24`)
  - count completions per hour
  - derive peak/hour and sparkline

## Implementation

- New module: `crates/forge-tui/src/analytics_dashboard.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
