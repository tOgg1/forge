# TUI-611 empty/loading/error states across panes

Task: `forge-1rj`
Status: delivered

## What shipped

- Added a tab-aware loading banner in the main renderer when `action_busy` is true.
- Banner text is pane-specific for clearer operator context:
  - Overview: `loading: refreshing loop inventory`
  - Logs: `loading: syncing selected loop logs`
  - Runs: `loading: syncing run timeline`
  - Multi Logs: `loading: syncing command-center lanes`
  - Inbox: `loading: syncing inbox + claim conflicts`
- Added regression test `action_busy_renders_loading_banner_for_active_tab` in `crates/forge-tui/src/app.rs`.

## Validation

- `cargo fmt --check`
- `cargo clippy -p forge-tui --all-targets -- -D warnings`
- `cargo test -p forge-tui`
