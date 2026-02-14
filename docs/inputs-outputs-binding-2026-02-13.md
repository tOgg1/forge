# Inputs/Outputs Binding (M3.2) - 2026-02-13

Implemented runtime input/output binding for workflow execution in `crates/forge-cli/src/workflow.rs`.

## What was added

- Input resolution for each step from `step.inputs`.
- Simple template engine for bindings:
  - `{{steps.<step_id>.<output_key>}}`
  - `{{inputs.<input_key>}}` (step-local context)
- Bash step input injection as environment variables:
  - `FORGE_INPUT_<INPUT_KEY>`
- Output capture map per completed step:
  - default keys: `output`, `stdout`, `stderr`, `exit_code`
  - plus explicit keys from `step.outputs`

## Error behavior

- Missing template step output now fails the current step with a clear message.
- Failure is persisted in step log/status (`failed`) and workflow run status.

## Additional runtime changes

- `BashStepRequest` supports `extra_env`.
- Workflow run path now records step failure consistently for template/input/output resolution errors.

## Verification

- `cargo test -p forge-cli --lib workflow::tests::run_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::run_persistence:: -- --nocapture`
- `cargo check -p forge-cli`
