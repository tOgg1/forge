# forge-3wv - forge-tui log_anchors expect-used slice

Date: 2026-02-14
Task: `forge-3wv`
Scope: `crates/forge-tui/src/log_anchors.rs`

## Change

- Removed `#[allow(clippy::expect_used)]` from test module.
- Replaced all test `expect(...)` callsites with explicit handling across the `log_anchors` tests.

## Validation

```bash
rg -n "expect\\(|expect_err\\(" crates/forge-tui/src/log_anchors.rs
cargo test -p forge-tui --lib log_anchors::tests
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'log_anchors.rs' || true
```

Result:
- No `expect(` / `expect_err(` remains in this file.
- Log anchors tests passed (`10 passed`).
- No `clippy::expect_used` diagnostics emitted for this file.

