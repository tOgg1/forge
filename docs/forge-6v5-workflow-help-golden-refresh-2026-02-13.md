# forge-6v5: workflow help golden refresh (2026-02-13)

## Summary
- Fixed `workflow_help_matches_golden` drift in `workflow_command_test`.
- Current workflow help includes additional subcommands/flags (`approve`, `deny`, `blocked`, and `--node/--step/--reason`).

## Changes
- Regenerated:
  - `crates/forge-cli/tests/golden/workflow/help.txt`

## Validation
- `cargo test -p forge-cli --test workflow_command_test` (4 passed)
