# forge-dqc: forge-cli job unwrap/expect test slice (2026-02-13)

## Scope
Remove clippy `unwrap_used` / `expect_used` / `expect_err` callsites in `crates/forge-cli/src/job.rs` tests.

## Changes
Converted reported callsites to explicit match-based handling:

- cron parser error assertions
- RFC3339 timestamp parses
- trigger create/tick/list/history operations
- append/list run operations
- webhook trigger create/remove operations

Formatted touched file.

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-cli/src/job.rs
cargo clippy -p forge-cli --all-targets -- -D warnings
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli --lib cron_parser_rejects_misconfigured_expression
cargo test -p forge-cli --lib webhook_trigger_can_be_created_and_removed
```

Results:

- full clippy still fails elsewhere, but no remaining `job.rs` diagnostics
- clippy slice with unwrap/expect allowed passed
- focused job tests passed
