# TUI-106 session restore and delta digest

Task: `forge-chf`  
Status: delivered

## Scope

- Added session snapshot model for restoring operator context:
  - selected loop
  - selected run
  - tab + layout
  - filter state/query
  - pane focus set
  - pinned loops
- Added privacy-safe persistence policy controls.
- Added delta digest generation to summarize changes since previous session.

## Policy controls (opt-out)

- `restore_enabled`: disable restore behavior completely.
- `persist_enabled`: disable snapshot persistence completely.
- `persist_filter_query`: keep filter query text only when explicitly enabled.
- `persist_selected_run`: allow/deny storing selected run id.

Defaults:
- restore on
- persist on
- filter query text off (digest only)
- selected run on

## Privacy-safe storage behavior

- Filter query text is redacted by default.
- Snapshot still stores deterministic digest (`filter_query_digest`) for change detection.
- Restore path emits notices when a field is intentionally omitted by policy.

## Restore behavior

- Applies stored context only when values are still valid in current universe.
- Falls back to default available tab/layout/pane when stored values are stale.
- Emits operator notices for:
  - missing loop/tab/layout/pane
  - privacy redactions

## Delta digest

- Compares current snapshot against previous snapshot.
- Reports deterministic context diffs for:
  - selected loop/run
  - tab/layout
  - filter state/query digest
  - pane focus set
  - pinned loops (+added / -removed)

## Implementation

- New module: `crates/forge-tui/src/session_restore.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
