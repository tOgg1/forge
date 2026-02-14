# forge-080 - interactive smoke kill confirm regression

Date: 2026-02-14
Task: `forge-080`
Scope:
- `crates/forge-tui/tests/interactive_smoke_test.rs`

## Change

- Updated `expanded_logs_kill_action_with_confirm` to match current confirm semantics for `Kill`:
  - verify reason is required in confirm state
  - move selection to confirm rail (`Tab`)
  - enter a minimum-length reason
  - submit via `Enter` instead of legacy `'y'` fast-accept path

## Validation

```bash
cargo test -p forge-tui --test interactive_smoke_test expanded_logs_kill_action_with_confirm
cargo test -p forge-tui --test interactive_smoke_test
```

Result:
- Targeted regression test passed.
- Full `interactive_smoke_test` suite passed (36 tests).
