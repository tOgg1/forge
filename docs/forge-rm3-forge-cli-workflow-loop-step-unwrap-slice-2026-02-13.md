# forge-rm3: forge-cli workflow loop-step unwrap-used slice (2026-02-13)

## Scope
Remove `unwrap/unwrap_err` callsites in `crates/forge-cli/src/workflow_run_persistence.rs` loop-step tests.

## Changes
Replaced 4 callsites with explicit success/error matches and panic messages:

- stop.expr success path
- stop.tool success path
- stop.tool failure path
- unsupported stop.llm failure path

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-cli/src/workflow_run_persistence.rs
cargo clippy -p forge-cli --all-targets -- -D warnings
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli --lib loop_step_expr_condition_stops_and_records_reason
cargo test -p forge-cli --lib loop_step_surfaces_stop_tool_failures
```

Results:

- full clippy still fails elsewhere, but no remaining `workflow_run_persistence.rs` diagnostics
- clippy slice with unwrap/expect allowed passed
- both focused loop-step tests passed
