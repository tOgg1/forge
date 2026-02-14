# forge-g71 - forge-tui runs_tab expect-used slice

Date: 2026-02-13
Task: `forge-g71`
Scope: `crates/forge-tui/src/runs_tab.rs`

## Change

- Replaced test `expect("output row should be rendered")` with explicit `match` + panic context in `paneled_output_applies_syntax_colors_to_tokens`.

## Validation

```bash
cargo test -p forge-tui --lib runs_tab::tests::paneled_output_applies_syntax_colors_to_tokens
rg -n "expect\\(" crates/forge-tui/src/runs_tab.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'runs_tab.rs' || true
```

Result:
- Targeted test passed.
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.

