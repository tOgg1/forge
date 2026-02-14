# forge-fr1 - forge-tui log_anchors_marker_test expect-used slice

Date: 2026-02-13
Task: `forge-fr1`
Scope: `crates/forge-tui/tests/log_anchors_marker_test.rs`

## Change

- Replaced all `expect(...)` callsites with explicit result handling:
  - first anchor insert
  - second anchor insert
  - source anchor insert during export/import test

## Validation

```bash
cargo test -p forge-tui --test log_anchors_marker_test
rg -n "expect\\(" crates/forge-tui/tests/log_anchors_marker_test.rs
cargo clippy -p forge-tui --tests -- -A warnings -W clippy::expect_used 2>&1 | rg 'log_anchors_marker_test.rs' || true
```

Result:
- Marker test target passed (`2 passed`).
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this test file.

