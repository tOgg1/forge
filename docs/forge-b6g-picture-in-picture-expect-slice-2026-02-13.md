# forge-b6g - forge-tui picture_in_picture expect-used slice

Date: 2026-02-13
Task: `forge-b6g`
Scope: `crates/forge-tui/src/picture_in_picture/tests.rs`

## Change

- Replaced both `focus_next_pip_window(...).expect("focus")` callsites with explicit `match` handling in `collapsed_window_renders_compact_lines_and_focus_cycle`.

## Validation

```bash
cargo test -p forge-tui --lib picture_in_picture::tests
rg -n "expect\\(" crates/forge-tui/src/picture_in_picture/tests.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'picture_in_picture/tests.rs' || true
```

Result:
- Picture-in-picture tests passed (`5 passed`).
- No `expect(` remains in this test file.
- No `clippy::expect_used` diagnostics emitted for this file.

