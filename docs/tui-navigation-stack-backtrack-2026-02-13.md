# TUI navigation stack + backtrack

Date: 2026-02-13  
Tasks: `forge-n3h`, `forge-jee`, `forge-jay`

## Scope delivered

- Added navigation return stack for cross-tab deep-link jumps.
- Added user backtrack shortcut in main mode:
  - `b` -> pop last deep-link return point and restore context.
- Deep-link path now pushes return point before jump:
  - search overlay accept -> `jump_to_search_target(...)` -> push + jump.
- Context restored on backtrack:
  - tab, selected loop, selected run, log source, log layer.

## Code

- `crates/forge-tui/src/app.rs`
  - `NavigationReturnPoint`
  - `nav_history` state + bounded stack (`MAX_NAV_HISTORY`)
  - `push_navigation_return_point()`
  - `pop_navigation_return_point()`
  - `jump_to_search_target(...)` now pushes history
  - main-mode key: `b` for backtrack
  - help text updated with `b` shortcut
  - tests:
    - `deep_link_jump_pushes_nav_history_and_b_backtracks`
    - `backtrack_key_without_history_is_noop_with_status`

## Validation

```bash
cargo test -p forge-tui deep_link_jump_pushes_nav_history_and_b_backtracks -- --nocapture
cargo test -p forge-tui backtrack_key_without_history_is_noop_with_status -- --nocapture
cargo test -p forge-tui paneled_overview_next_action_slot_ -- --nocapture
cargo build -p forge-tui
```
