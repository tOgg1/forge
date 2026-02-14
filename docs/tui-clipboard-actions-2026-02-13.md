# TUI Clipboard Actions

Task: `forge-8z6`  
Date: `2026-02-13`  
Status: delivered

## Scope

- Add clipboard actions for primary operator contexts:
  - Runs: selected run ID
  - Logs: current log line (scroll-aware)
  - Inbox: selected thread content (latest body/subject)
- Add graceful fallback when OS clipboard command is unavailable.
- Keep copied text mirrored in app state for deterministic behavior/tests.

## Implementation anchors

- Clipboard state + accessor:
  - `crates/forge-tui/src/app.rs:847`
  - `crates/forge-tui/src/app.rs:1228`
- Context-aware copy action:
  - `crates/forge-tui/src/app.rs:1901`
- Keybinding path (`Ctrl+Y` in main mode):
  - `crates/forge-tui/src/app.rs:3232`
- Help text update:
  - `crates/forge-tui/src/app.rs:5975`
- Clipboard execution with fallback chain (`pbcopy`, `wl-copy`, `xclip`, `xsel`):
  - `crates/forge-tui/src/app.rs:6136`
  - `crates/forge-tui/src/app.rs:6146`

## Regression tests

- `crates/forge-tui/src/app.rs:6884`
- `crates/forge-tui/src/app.rs:6908`
- `crates/forge-tui/src/app.rs:6925`

## Validation

- `cargo test -p forge-tui --lib ctrl_y_copies_ -- --nocapture`
- `cargo test -p forge-tui --lib deep_link_jump_pushes_nav_history_and_b_backtracks -- --nocapture`
- `cargo build -p forge-tui`
