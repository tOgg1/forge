# TUI-918 predictive queue ETA estimator

Task: `forge-xep`  
Status: delivered

## Scope

- Estimate queue clear ETA from:
  - queue depth
  - historical completion throughput
  - run duration samples
- Emit reliability state + warnings for degraded conditions.
- Track ETA quality metric (`within 20%` ratio).

## Implementation

- New module: `crates/forge-tui/src/predictive_queue_eta.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

### Estimator API

- `estimate_queue_eta(...)` returns:
  - `QueueEtaState` (`Empty|Healthy|Risky|Stalled`)
  - `eta_seconds` + display string
  - blended throughput (`runs/min`)
  - error rate
  - confidence score
  - warnings (`no active workers`, `high failure rate`, low-confidence, threshold issues)
- Throughput model:
  - aggregate historical completion rate
  - derive duration-based worker throughput
  - blend both sources with sample-volume weighting

### Accuracy API

- `evaluate_eta_accuracy(...)` computes:
  - total samples
  - samples within 20% relative error
  - ratio within 20%

## Validation

- `cargo fmt --all`
- `cargo build -p forge-tui`
- `cargo test -p forge-tui predictive_queue_eta::` may be blocked if unrelated in-flight compile failures exist in other touched crates/modules.
