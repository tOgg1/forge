# forge-j59: forge-cli completion expect-used slice (2026-02-13)

## Scope
Remove clippy `expect_used` callsites in `crates/forge-cli/src/completion.rs` test helper/setup.

## Changes
Replaced `expect(...)` with explicit error handling in:

- `write_executable` (`write`, `metadata`, `set_permissions`)
- completion editor probe temp-dir creation

Behavior preserved; errors now include explicit panic context.

## Validation
Commands run:

```bash
cargo clippy -p forge-cli --all-targets -- -D warnings
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli --lib completion_generation_does_not_invoke_editor
```

Results:

- full clippy still fails elsewhere, but no remaining `completion.rs` diagnostics
- clippy slice with unwrap/expect allowed passed
- focused completion test passed
