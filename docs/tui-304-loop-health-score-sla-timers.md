# TUI-304 loop health score and SLA timers

Task: `forge-wzb`  
Status: delivered

## Scope

- Compute per-loop health score from:
  - liveness
  - queue age
  - error rate
  - run recency
- Surface SLA timer deltas and explicit breach reasons.
- Infer probable causes for degraded/critical loops.

## Implementation

- New module: `crates/forge-tui/src/loop_health_score.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

### Core API

- `compute_loop_health_scores(samples)`
  - emits deterministic worst-first health ranking
  - returns score + health label (`healthy|degraded|critical`)
  - includes timer/budget remaining fields:
    - `queue_sla_remaining_s`
    - `run_sla_remaining_s`
    - `error_budget_remaining_pct`
  - lists `sla_breaches` + `probable_causes`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
