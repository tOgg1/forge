# forge-xtj - forge-tui quick_peek_popups expect-used slice

Date: 2026-02-14
Task: `forge-xtj`
Scope: `crates/forge-tui/src/quick_peek_popups.rs`

## Change

- Replaced all `expect(...)` callsites with explicit `match` handling in:
  - `loop_peek_contains_health_task_and_output`
  - `task_peek_contains_status_assignee_description`
  - `file_peek_shows_first_twenty_lines_and_recent_changes`
  - `fmail_peek_shows_latest_message`
  - `commit_peek_uses_short_hash`

## Validation

```bash
cargo test -p forge-tui --lib quick_peek_popups::tests
rg -n "expect\\(" crates/forge-tui/src/quick_peek_popups.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'quick_peek_popups.rs' || true
```

Result:
- Quick peek popup tests passed (`7 passed`).
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.

