# forge-1rd: forge-cli mesh expect-used slice (2026-02-13)

## Scope
Remove clippy `expect_used`/`expect_err` callsites in `crates/forge-cli/src/mesh.rs` tests.

## Changes
Converted reported callsites to explicit handling:

- node profile provisioning/reporting checks
- status load checks
- node lookups
- invalid auth-state error assertion path

Formatted touched file.

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-cli/src/mesh.rs
cargo clippy -p forge-cli --all-targets -- -D warnings
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli --lib provision_and_report_auth_aggregates_node_and_mesh_auth_status
cargo test -p forge-cli --lib report_auth_rejects_invalid_state
```

Results:

- full clippy still fails elsewhere, but no remaining `mesh.rs` diagnostics
- clippy slice with unwrap/expect allowed passed
- both focused mesh tests passed
