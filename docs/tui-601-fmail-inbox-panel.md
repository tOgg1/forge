# TUI-601 fmail inbox panel

Status: implemented in `crates/forge-tui/src/app.rs`.

Scope delivered:
- New `Inbox` main tab (`5`) in Forge TUI shell.
- Threaded inbox grouping using CLI-parity thread key semantics:
  - use `thread_id` when present
  - fallback thread key to message id (`m-<id>`)
- CLI-parity message id display: `m-<id>`.
- Inbox filters: `all`, `unread`, `ack-required` (`f` to cycle).
- Unread tracking in thread list and detail pane.
- Quick actions in Inbox tab:
  - `enter`: mark selected thread read
  - `a`: acknowledge latest pending ack in selected thread
  - `r`/`R`: quick reply intent shortcut with thread + reply-to context

Operator notes:
- Inbox UI is modeled in app state first; action execution currently updates local TUI state/status.
- Parity preserved for id/thread display contracts used by `forge mail`.
