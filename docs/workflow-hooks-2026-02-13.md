# Workflow Hooks (M3.3) - 2026-02-13

Implemented step/workflow hook execution for `forge workflow run` in `crates/forge-cli/src/workflow.rs`.

## Hook format

- Supported hook type: `bash` (initial)
- Syntax:
  - `bash:<cmd>` (default fail mode)
  - `fail:bash:<cmd>`
  - `warn:bash:<cmd>`

## Execution behavior

- Workflow hooks:
  - `hooks.pre` run once before step execution.
  - `hooks.post` run once after step execution.
- Step hooks:
  - `step.hooks.pre` run before step body.
  - `step.hooks.post` run after step body attempt.
- Hook output (`stdout`/`stderr`/exit) is logged to the step log.

## Failure policy

- `fail` mode: non-zero hook exit fails the step/workflow.
- `warn` mode: non-zero hook exit logs warning and execution continues.
- Workflow pre-hook failure marks run failed and returns failed run status.

## Verification

- `cargo test -p forge-cli --lib workflow::tests::run_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::run_hook -- --nocapture`
- `cargo check -p forge-cli`
