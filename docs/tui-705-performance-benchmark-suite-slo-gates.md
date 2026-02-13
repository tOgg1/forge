# TUI-705 performance benchmark suite and SLO gates

Tasks: `forge-98e`, `forge-p6h`  
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
- `run_benchmark_case_with_work_units(...)` supports throughput budgets in domain units (for follow-mode line throughput gates).
- `ViewSlo` defines SLO thresholds:
  - max p50 latency
  - max p95 latency
  - min throughput
- `evaluate_slo_gates(...)` returns `SloGateReport` with missing views and metric breaches.
- `format_ci_gate_summary(...)` emits CI-friendly PASS/FAIL output.

## CI regression gates (hard fail)

- Render gate test: `ci_render_latency_and_throughput_budgets_hold` (`crates/forge-tui/src/performance_gates.rs`)
- Follow gate test: `ci_follow_throughput_budget_holds` (`crates/forge-tui/src/performance_gates.rs`)
- Budgets:
  - `overview/logs/runs`: p50 `<=18ms`, p95 `<=40ms`, throughput `>=45 fps`
  - `multi-logs`: p50 `<=35ms`, p95 `<=70ms`, throughput `>=25 fps`
  - `follow`: p50 `<=12ms`, p95 `<=24ms`, throughput `>=12,000 lines/s`

These tests run in normal `cargo test -p forge-tui`, so CI fails immediately on perf regression.

## Validation

- Unit tests in `crates/forge-tui/src/performance_gates.rs` cover:
  - default suite coverage
  - benchmark sample generation
  - throughput scaling with explicit work units
  - pass/fail gate behavior
  - missing-view detection
  - CI summary formatting
  - suite persist/restore normalization and dedupe handling
  - render-latency + follow-throughput regression gates
