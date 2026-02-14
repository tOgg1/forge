# forge-2aq: forge-cli trigger expect-used slice (2026-02-13)

## Scope
Remove clippy `expect_used` callsites in `crates/forge-cli/src/trigger.rs` tests.

## Changes
Converted four `expect(...)` sites to explicit error handling with `if let` / `match` + panic context:

- 3x `create_job`
- 1x `list_triggers`

## Validation
Commands run:

```bash
cargo clippy -p forge-cli --all-targets -- -D warnings
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli --lib add_list_remove_cron_trigger
```

Results:

- full clippy still fails elsewhere, but no remaining `trigger.rs` diagnostics
- clippy slice with unwrap/expect allowed passed
- focused trigger unit test passed
