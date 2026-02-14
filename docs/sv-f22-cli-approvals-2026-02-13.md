# sv-f22: Workflow CLI Approvals

Date: 2026-02-13
Task: `sv-f22`

## Scope implemented

Implemented workflow human-step approval CLI paths in `forge-cli`:

- New subcommands in `workflow` command surface:
  - `forge workflow approve <run-id> --step <step-id>`
  - `forge workflow deny <run-id> --step <step-id> --reason <text>`
  - `forge workflow blocked <run-id>`
- Parser and help text updated in `crates/forge-cli/src/workflow.rs`.

Added persistence transition support in `crates/forge-cli/src/workflow_run_persistence.rs`:

- `WorkflowRunStore::decide_step_approval(...)`
  - validates waiting-approval state
  - marks approval metadata (`Approved`/`Rejected`, `decided_at`)
  - updates step status (`Success`/`Failed`)
  - on deny: marks active pending/running/waiting steps as skipped and fails run
  - appends audit log line with deny reason when provided

Blocked-step listing behavior:

- `workflow blocked` resolves current run + workflow graph and reports steps blocked by:
  - waiting human approval
  - dependency steps not in `success`

## Regression tests added

`crates/forge-cli/src/workflow.rs`:

- `approve_command_marks_waiting_step_approved`
- `deny_command_fails_run_and_records_reason`
- `blocked_command_lists_waiting_and_dependency_blocked_steps`
- `approve_missing_step_usage_error`
- `deny_missing_reason_usage_error`

`crates/forge-cli/src/workflow_run_persistence.rs`:

- `approve_waiting_step_marks_step_success_and_keeps_run_running`
- `deny_waiting_step_marks_run_failed_and_skips_remaining_steps`

## Validation

Executed:

- `cargo check -p forge-cli`
- `cargo test -p forge-cli --lib workflow::tests::approve_command_marks_waiting_step_approved`
- `cargo test -p forge-cli --lib workflow::tests::deny_command_fails_run_and_records_reason`
- `cargo test -p forge-cli --lib workflow::tests::blocked_command_lists_waiting_and_dependency_blocked_steps`
- `cargo test -p forge-cli --lib workflow::tests::approve_missing_step_usage_error`
- `cargo test -p forge-cli --lib workflow::tests::deny_missing_reason_usage_error`
- `cargo test -p forge-cli --lib workflow::run_persistence::tests::approve_waiting_step_marks_step_success_and_keeps_run_running`
- `cargo test -p forge-cli --lib workflow::run_persistence::tests::deny_waiting_step_marks_run_failed_and_skips_remaining_steps`
