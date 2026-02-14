# forge-bz7 - forge-tui bin/forge-tui expect-used slice

Date: 2026-02-14
Task: `forge-bz7`
Scope: `crates/forge-tui/src/bin/forge-tui.rs`

## Change

- Removed test-module blanket lint allow:
  - `#![allow(clippy::expect_used, clippy::unwrap_used)]`
- Added test helper:
  - `ok_or_panic<T, E: Display>(Result<T, E>, context)`
- Replaced all test `expect(...)` callsites with explicit `ok_or_panic(...)` handling in:
  - `live_snapshot_includes_loop_queue_and_profile_fields`
  - `live_snapshot_refreshes_queue_depth_after_enqueue`
  - `snapshot_renders_team_summary_and_task_inbox_sections`

## Validation

```bash
rg -n "expect\\(|expect_err\\(|unwrap\\(" crates/forge-tui/src/bin/forge-tui.rs
cargo clippy -p forge-tui --bin forge-tui --tests -- -D clippy::expect_used -D clippy::unwrap_used
cargo test -p forge-tui --bin forge-tui
```

Result:
- No `expect(` / `expect_err(` / `unwrap(` callsites remain in `bin/forge-tui.rs`.
- Focused clippy run passed for `expect_used` and `unwrap_used`.
- `forge-tui` bin tests passed (9/9).
