# forge-dx0 - forge-tui multi_logs unwrap-used slice

Date: 2026-02-14
Task: `forge-dx0`
Scope: `crates/forge-tui/src/multi_logs.rs`

## Change

- Replaced three test `unwrap()` callsites on `frame.cell(...)` with explicit `match` handling in `compare_mode_renders_row_level_diff_hints`.

## Validation

```bash
cargo test -p forge-tui --lib multi_logs::tests::compare_mode_renders_row_level_diff_hints
rg -n "unwrap\\(" crates/forge-tui/src/multi_logs.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::unwrap_used 2>&1 | rg 'multi_logs.rs' || true
```

Result:
- Targeted multi-logs test passed.
- No `unwrap(` remains in this file.
- No `clippy::unwrap_used` diagnostics emitted for this file.

