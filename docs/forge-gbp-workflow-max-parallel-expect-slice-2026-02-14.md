# forge-gbp - workflow max-parallel + write_executable expect slice

Date: 2026-02-14
Task: `forge-gbp`
Scope: `crates/forge-cli/src/workflow.rs`

## Change

- Replaced `write_executable` helper `expect(...)` callsites with explicit handling:
  - write
  - metadata/stat
  - chmod/set_permissions
- Replaced `resolve_workflow_max_parallel` test `unwrap/unwrap_err` callsites with explicit `match` handling in:
  - `resolve_workflow_max_parallel_prefers_workflow_field`
  - `resolve_workflow_max_parallel_uses_env_when_workflow_unset`
  - `resolve_workflow_max_parallel_uses_global_config`
  - `resolve_workflow_max_parallel_rejects_invalid_env`

## Validation

```bash
rg -n "write executable|stat executable|chmod executable|resolve_workflow_max_parallel\\(&wf\\)\\.unwrap\\(|resolve_workflow_max_parallel\\(&basic_workflow\\(\\)\\)\\.unwrap_err\\(" crates/forge-cli/src/workflow.rs
cargo test -p forge-cli --lib resolve_workflow_max_parallel_
cargo test -p forge-cli --lib workflow_run_routes_through_master_node_locally
cargo test -p forge-cli --lib workflow_run_reports_master_offline_when_ssh_probe_fails
```

Result:
- Targeted old `expect/unwrap` patterns were removed from this slice.
- Focused tests passed.
