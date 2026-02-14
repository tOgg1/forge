# Human step model + persistence (sv-znr)

Date: 2026-02-13
Task: sv-znr (M4.1)

## Delivered

- Added explicit human pause step state:
  - `waiting_approval` in persisted step status.
- Added persisted approval metadata per step (`run.json`):
  - `approval.state` (`pending|approved|rejected|timed_out`)
  - `approval.requested_at`
  - `approval.decided_at` (optional)
  - `approval.timeout_at` (optional)
- Added store API for pause transition:
  - `mark_step_waiting_approval(run_id, step_id, timeout_at)`
- Extended parallel scheduler with pause outcome:
  - step execution can return `WaitingApproval`
  - scheduler marks step `waiting_approval`
  - scheduler stops launching new work and preserves remaining pending steps.
- Workflow run behavior:
  - `human` steps now pause run (status remains `running`)
  - dependent steps remain pending
  - workflow post hooks are not executed while paused.

## Human timeout behavior

- `human.timeout` supports `<int>[s|m|h|d]`.
- If omitted, default timeout is `24h`.
- `workflow show` surfaces timeout as:
  - `timeout: <value>` when configured
  - `timeout: default(24h)` when omitted.
- Runtime writes approval wait log line with timeout context.

## Validation

- Human timeout literals are validated in workflow validation.
- Invalid literals produce `timeout` field validation errors.

## Tests

- `cargo test -p forge-cli --lib workflow::run_persistence::tests:: -- --nocapture`
- `cargo test -p forge-cli --lib workflow::run_persistence::engine_tests::parallel_workflow_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::run_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::show_workflow_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::logs_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::validate_human_timeout_literal -- --nocapture`
- `cargo check -p forge-cli`
