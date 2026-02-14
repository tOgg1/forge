# forge-zxx: forge-cli registry unwrap-used slice (2026-02-13)

## Scope
Remove `unwrap` usage in `crates/forge-cli/src/registry.rs` test path (`export_writes_commit_friendly_registry_file`).

## Changes
Replaced three `unwrap` call sites with explicit error handling:

- directory creation
- prompt fixture file write
- registry status load

All now emit precise `panic!` context on failure.

## Validation
Commands run:

```bash
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli export_writes_commit_friendly_registry_file
```

Results:

- clippy slice passed
- target registry test passed
