# Workflow Show: Blocked Status + Approval Hints (sv-5af)

Task: `sv-5af`

## Summary
- `forge workflow show <name>` now surfaces blocked/approval context in step details.
- Human steps explicitly show:
  - `blocked: yes`
  - `reason: awaiting human approval`
  - `reason: approval timeout: <value>` (when configured)
  - `reason: approve via: forge workflow approve <run-id> --step <id>`
- Steps depending on a human gate now show blocked dependency reason and approval hint.
- Timeout values are printed in step details (`timeout: ...`) for visibility.

## Code
- `crates/forge-cli/src/workflow.rs`
  - Added blocked-reason derivation for workflow steps.
  - Wired blocked reason output into `print_workflow` step detail rendering.
  - Included `WaitingApproval` status handling in status labeling and run reconciliation.
  - Added/updated human-timeout helper usage to support waiting-approval path.
- `crates/forge-cli/src/workflow_run_persistence.rs`
  - Added explicit type annotation for collected step records in workflow engine run assembly.

## Tests
- Added:
  - `workflow::tests::show_workflow_surfaces_blocked_human_step_reason_and_timeout`
- Validation commands:
  - `cargo fmt --package forge-cli`
  - `cargo test -p forge-cli show_workflow_surfaces_blocked_human_step_reason_and_timeout -- --nocapture`
  - `cargo build -p forge-cli`
