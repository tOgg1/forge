# forge-agp: forge-cli webhook server + final clippy cleanup (2026-02-13)

## Scope
Primary scope: remove clippy `unwrap_used` / `expect_used` callsites in `crates/forge-cli/src/webhook_server.rs`.

Follow-up discovered during validation: one remaining `expect_used` in
`crates/forge-cli/tests/profile_command_test.rs` lock acquisition helper.

## Changes
### `crates/forge-cli/src/webhook_server.rs`
- replaced single-trigger resolution `.expect(...)` with explicit `match`
- replaced test `expect(...)` callsites for:
  - gate-response assertions
  - RFC3339 parse paths
  - job/trigger creation
  - run listing
  - duplicate trigger setup

### `crates/forge-cli/tests/profile_command_test.rs`
- replaced lock acquisition `.expect(...)` with explicit match + panic message

Also formatted touched webhook file.

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-cli/src/webhook_server.rs
cargo test -p forge-cli --lib routed_webhook_records_job_run_and_returns_run_id
cargo test -p forge-cli --lib duplicate_webhook_path_returns_409
cargo clippy -p forge-cli --all-targets -- -D warnings
cargo test -p forge-cli --test profile_command_test
```

Results:

- focused webhook tests passed
- full `cargo clippy -p forge-cli --all-targets -- -D warnings` passed
- profile command integration test passed
