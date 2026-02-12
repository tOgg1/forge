# TUI-603 handoff snapshot generator

Task: `forge-nse`
Status: delivered

## What shipped

- Inbox handoff hotkey: `h` (tab `Inbox`).
- Generates compact handoff package for selected thread/task/loop with:
  - `status`
  - `context`
  - `links`
  - `pending-risks`
- Package rendered in Inbox detail pane as `handoff snapshot`.
- Task/loop inference:
  - task id from thread content (`forge-*`), then conflict/claim fallback
  - loop id from thread content (`loop-*`), then selected loop fallback
- Risk synthesis includes unread/ack backlog, ownership conflict, and loop-state issues.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
