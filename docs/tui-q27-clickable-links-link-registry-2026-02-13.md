# TUI-Q27 clickable links via link registry

## Scope
- Add a lightweight link registry for actionable references found in TUI text.
- Provide in-app activation path with graceful fallback behavior.

## Changes
- Added `crates/forge-tui/src/link_registry.rs`:
  - `LinkTarget` (`Url`, `Run`, `Loop`)
  - `LinkRegistry` extraction + de-dup from plain text
  - punctuation-safe token trimming
  - unit tests for extraction/de-dup behavior
- Updated `crates/forge-tui/src/app.rs`:
  - integrate context link collection (`Logs`, `Runs`, `Overview`, `MultiLogs`, `Inbox` handoff lines)
  - add `Ctrl+O` primary-link activation path
  - run/loop links jump context via existing deep-link navigation
  - URL links produce copy/paste fallback status message
  - help text includes `Ctrl+O` hint
  - app tests for `Ctrl+O` loop jump, URL fallback, and empty-context behavior
- Updated `crates/forge-tui/src/lib.rs` to export `link_registry` module.

## Validation
- `cargo test -p forge-tui link_registry::tests:: -- --nocapture`
- `cargo test -p forge-tui ctrl_o_ -- --nocapture`
- `cargo build -p forge-tui`
