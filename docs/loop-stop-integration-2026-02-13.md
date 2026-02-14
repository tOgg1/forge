# Loop Stop Integration (M3.1c) - 2026-02-13

Integrated stop-condition evaluation into workflow loop-step executor primitives in `crates/forge-cli/src/workflow_run_persistence.rs`.

## Added

- `LoopStepStopCondition`
  - supports `expr`, `tool`, and `has_llm_condition` flag (LLM currently rejected).
- `LoopStepStopEvaluation`
  - explicit `should_stop` + `reason`.
- `evaluate_loop_stop_condition(...)`
  - evaluates `stop.expr` using `count(tasks.open)` provider.
  - evaluates `stop.tool` via `forge_loop::stop_rules::run_stop_tool`.
  - returns clear error for unsupported `stop.llm`.
- `execute_loop_step_with_stop_condition(...)`
  - wires stop-condition evaluation into loop iteration execution.

## Behavior

- Loop step stops early when `stop.expr` or `stop.tool` matches.
- Stop reason is recorded on result (`LoopStepExecutionResult.stop_reason`) and in loop logs.
- Stop-tool failures (spawn/timeout) bubble as explicit errors.
- Iteration failure and max-iteration terminal paths now carry explicit reasons.

## Verification

- `cargo test -p forge-cli --lib workflow::run_persistence::loop_step_tests:: -- --nocapture`
- `cargo test -p forge-cli --lib workflow::run_persistence:: -- --nocapture`
- `cargo check -p forge-cli`
