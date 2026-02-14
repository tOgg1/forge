# forge-c11 - forge-tui status_strip expect-used slice

Date: 2026-02-14
Task: `forge-c11`
Scope: `crates/forge-tui/src/status_strip.rs`

## Change

- Replaced all test `expect`/`expect_err` callsites with explicit handling in:
  - `register_pluggable_widget_and_persist_round_trip`
  - `register_duplicate_widget_id_is_rejected`
  - `move_widget_between_strips_reorders_slots`
  - `set_widget_enabled_toggles_visibility`

## Validation

```bash
cargo test -p forge-tui --lib status_strip::tests
rg -n "expect\\(|expect_err\\(" crates/forge-tui/src/status_strip.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'status_strip.rs' || true
```

Result:
- Status strip tests passed (`9 passed`).
- No `expect(` / `expect_err(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.

