# forge-4te - workflow node-flag expect slice

Date: 2026-02-14
Task: `forge-4te`
Scope: `crates/forge-cli/src/workflow.rs`

## Change

- Replaced `expect`/`unwrap_err` in node-flag parse tests with explicit `match` handling:
  - `parse_run_accepts_node_flag`
  - `parse_node_flag_rejected_for_non_run_subcommands`
- Replaced `create_dir_all(...).expect("create bin dir")` with explicit error handling in:
  - `workflow_run_routes_through_master_node_locally`
  - `workflow_run_reports_master_offline_when_ssh_probe_fails`

## Validation

```bash
rg -n "expect\\(\"parse workflow run --node\"\\)|create_dir_all\\(&bin_dir\\)\\.expect\\(\"create bin dir\"\\)|parse_node_flag_rejected_for_non_run_subcommands\\(\\).*unwrap_err" crates/forge-cli/src/workflow.rs
cargo test -p forge-cli --lib node_flag
cargo test -p forge-cli --lib workflow_run_routes_through_master_node_locally
cargo test -p forge-cli --lib workflow_run_reports_master_offline_when_ssh_probe_fails
```

Result:
- Targeted `expect`/`unwrap_err` patterns are removed from this slice.
- Focused test runs passed.
