# forge-sh6: forge-cli external_adapter expect-used slice (2026-02-13)

## Scope
Remove clippy `expect_used` callsites in `crates/forge-cli/src/external_adapter.rs` test helpers/tests.

## Changes
Converted helper/test `expect(...)` calls to explicit handling:

- DB open/migrate helper
- team seed helper
- ingest result checks
- team-task list result checks
- multi-adapter ingest result

Formatted touched file.

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-cli/src/external_adapter.rs
cargo clippy -p forge-cli --all-targets -- -D warnings
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli --lib ingest_enabled_adapters_collects_per_adapter_results
```

Results:

- full clippy still fails elsewhere, but no remaining `external_adapter.rs` diagnostics
- clippy slice with unwrap/expect allowed passed
- focused external-adapter test passed
