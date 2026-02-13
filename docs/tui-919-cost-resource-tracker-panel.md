# TUI-919 cost/resource tracker panel

Task: `forge-tw6`  
Status: delivered

## Scope

- Live panel model for:
  - token usage
  - API call volume
  - compute time
  - cost burn rate
  - CPU/memory pressure
- Trend + anomaly alerts for fast operator detection.

## Implementation

- New module: `crates/forge-tui/src/cost_resource_tracker.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

### Core model

- `ResourceSample`: timestamped usage sample.
- `ResourceBudgetPolicy`: burn-rate, spike, CPU/memory thresholds.
- `build_resource_tracker_summary(...)`:
  - aggregates totals/rates
  - computes trends (`Up|Flat|Down`)
  - builds token/cost sparklines
  - emits sorted alerts (`CostBurnRate`, `TokenSpike`, `ApiRateSpike`, `CpuPressure`, `MemoryPressure`)
- `render_resource_tracker_panel_lines(...)`: deterministic panel text lines.

### Alert behavior

- Burn-rate alerts from latest hourly projection.
- CPU/memory threshold alerts from latest sample.
- Token/API spike alerts vs rolling baseline when history depth is sufficient.

## Validation

- `cargo fmt --all`
- `cargo build -p forge-tui`
- `cargo test -p forge-tui cost_resource_tracker::`
