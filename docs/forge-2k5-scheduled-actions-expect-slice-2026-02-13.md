# forge-2k5 - forge-tui scheduled_actions expect-used slice

Date: 2026-02-13
Task: `forge-2k5`
Scope: `crates/forge-tui/src/scheduled_actions.rs`

## Change

- Replaced all test `expect(...)` callsites with explicit handling in:
  - `schedule_after_and_pop_due_actions_in_order`
  - `status_line_shows_timer_count_and_countdown`
  - `cancel_removes_item_by_schedule_id`
  - `due_actions_peek_does_not_pop`

## Validation

```bash
cargo test -p forge-tui --lib scheduled_actions::tests
rg -n "expect\\(" crates/forge-tui/src/scheduled_actions.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'scheduled_actions.rs' || true
```

Result:
- Scheduled actions tests passed (`5 passed`).
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.

