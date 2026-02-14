# forge-p49 - forge-tui watchpoints expect-used slice

Date: 2026-02-13
Task: `forge-p49`
Scope: `crates/forge-tui/src/watchpoints.rs`

## Change

- Replaced `expect("restore")` with explicit `match` handling in `persist_and_restore_round_trip`.

## Validation

```bash
cargo test -p forge-tui --lib watchpoints::tests::persist_and_restore_round_trip
rg -n "expect\\(" crates/forge-tui/src/watchpoints.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'watchpoints.rs' || true
```

Result:
- Targeted test passed.
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.

