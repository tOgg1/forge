# TUI-705 performance benchmark suite and SLO gates

Task: `forge-98e`  
Status: delivered

## Scope

- Define latency + throughput SLOs for key TUI views.
- Provide benchmark suite configuration for automated runs.
- Provide deterministic gate evaluation and CI summary output.

## Implementation

- New module: `crates/forge-tui/src/performance_gates.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Bench suite model

- `BenchmarkSuite` + `BenchmarkCase` describe benchmark workload by view.
- Defaults include key views: `overview`, `logs`, `runs`, `multi-logs`.
- JSON persistence + restore:
  - `persist_benchmark_suite(...)`
  - `restore_benchmark_suite(...)`

## Runtime measurement + gates

- `run_benchmark_case(...)` captures per-iteration latency samples + derived throughput.
- `ViewSlo` defines SLO thresholds:
  - max p50 latency
  - max p95 latency
  - min throughput
- `evaluate_slo_gates(...)` returns `SloGateReport` with missing views and metric breaches.
- `format_ci_gate_summary(...)` emits CI-friendly PASS/FAIL output.

## Validation

- Unit tests in `crates/forge-tui/src/performance_gates.rs` cover:
  - default suite coverage
  - benchmark sample generation
  - pass/fail gate behavior
  - missing-view detection
  - CI summary formatting
  - suite persist/restore normalization and dedupe handling
