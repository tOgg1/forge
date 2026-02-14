# forge-e32: forge-tui run_output_diff expect-used slice (2026-02-13)

## Scope
Replace `expect_used` in `crates/forge-tui/src/run_output_diff.rs` regex helper.

## Changes
Updated `replace_all` to use explicit `match` on `Regex::new(pattern)` with panic context instead of `.expect(...)`.

Formatted touched file.

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-tui/src/run_output_diff.rs
cargo test -p forge-tui --lib semantic_normalization_suppresses_timestamp_duration_and_ids
cargo clippy -p forge-tui --all-targets -- -D warnings
```

Results:

- focused run-output-diff test passed
- full forge-tui clippy still fails elsewhere, but `run_output_diff.rs` no longer appears in diagnostics
