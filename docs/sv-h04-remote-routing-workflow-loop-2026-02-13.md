# sv-h04: Remote routing for workflow/loop (2026-02-13)

## Scope delivered

- Added `forge node` command family in `crates/forge-cli/src/node.rs`.
- Implemented `forge node ls` and `forge node exec <node> -- <command>`.
- `node exec` routing logic:
  - Reads mesh registry status.
  - Routes non-master targets through master (`client -> master -> target`).
  - Supports direct execution on master/local endpoints.
  - Surfaces explicit offline/unreachable errors (`master`/`node` endpoint missing or SSH `255`).
- Added remote passthrough hooks:
  - `forge workflow run <name> --node <node-id>`.
  - `forge run <loop> --node <node-id>`.
  - `forge loop run <loop> --node <node-id>`.
- Updated CLI docs with `workflow run --node` example.

## Tests and validation

- `cargo fmt --package forge-cli`
- `cargo test -p forge-cli node::tests:: -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::workflow_run_routes_through_master_node_locally -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::workflow_run_reports_master_offline_when_ssh_probe_fails -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::parse_run_accepts_node_flag -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::parse_node_flag_rejected_for_non_run_subcommands -- --nocapture`
- `cargo test -p forge-cli --lib parse_accepts_node_flag -- --nocapture`
- `cargo test -p forge-cli --lib parse_rejects_node_flag_without_value -- --nocapture`
- `cargo check -p forge-cli`

## Notes

- During verification, compile was blocked by missing helper functions in the currently untracked `crates/forge-cli/src/registry.rs`; restored those helpers so `forge-cli` builds cleanly.
