# Workflow CLI approvals (sv-f22)

Date: 2026-02-13
Task: sv-f22 (M4.2)

## Delivered

- Added CLI commands:
  - `forge workflow approve <run-id> --step <step-id>`
  - `forge workflow deny <run-id> --step <step-id> --reason <text>`
  - `forge workflow blocked <run-id>`
- Added positional fallback support for step/reason in parser:
  - `approve <run-id> <step-id>`
  - `deny <run-id> <step-id> <reason>`
- `approve` behavior:
  - decides waiting approval step
  - resumes workflow execution immediately
  - executes remaining pending steps (parallel scheduler path)
  - preserves and reuses persisted step outputs for input template resolution
  - ends `success` when remaining work completes
  - pauses again if another human step is reached
- `deny` behavior:
  - marks approved step as rejected/failed
  - skips remaining pending/waiting steps
  - marks workflow run failed
  - records denial reason in step log
- `blocked` behavior:
  - lists waiting approval steps and pending steps blocked by dependency status.

## Persistence updates

- Step run model now persists computed outputs (`step.outputs` map) for resume continuity.
- Approval decision API supports state transitions and audit line logging.

## Tests

- `cargo test -p forge-cli --lib workflow::tests::approve_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::deny_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::blocked_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::run_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::show_workflow_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::logs_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::run_persistence::tests:: -- --nocapture`
- `cargo test -p forge-cli --lib workflow::run_persistence::engine_tests::parallel_workflow_ -- --nocapture`
- `cargo check -p forge-cli`
