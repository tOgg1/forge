# forge-gzw - forge-tui view_export unwrap-used slice

Date: 2026-02-14
Task: `forge-gzw`
Scope: `crates/forge-tui/src/view_export.rs`

## Change

- Replaced test `unwrap()` callsites with explicit handling in `export_writes_txt_html_and_svg_files`:
  - `result.unwrap()`
  - `fs::read_to_string(...).unwrap()`
- Removed stale module attribute `#[allow(clippy::unwrap_used)]` from `view_export` tests.

## Validation

```bash
cargo test -p forge-tui --lib view_export::tests::export_writes_txt_html_and_svg_files
rg -n "unwrap\\(" crates/forge-tui/src/view_export.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::unwrap_used 2>&1 | rg 'view_export.rs' || true
cargo clippy -p forge-tui --all-targets -- -D clippy::unwrap_used -D clippy::expect_used
cargo test -p forge-tui view_export::tests
```

Result:
- Targeted view export test passed.
- No `unwrap(` remains in this file.
- No `clippy::unwrap_used` diagnostics emitted for this file.
- Focused clippy run with `-D clippy::unwrap_used -D clippy::expect_used` passed for `forge-tui`.
