# TUI-703 crash-safe state persistence and recovery

Task: `forge-nwk`  
Status: delivered

## Scope

- Added crash-safe snapshot persistence for minimal TUI session state.
- Added recovery path with backup fallback when primary snapshot is corrupt/unreadable.
- Added integrity guard (`snapshot_digest`) to reject tampered/partial snapshots.

## State model

- Reuses privacy-safe session snapshot fields from `session_restore`:
  - selected loop/run
  - tab + layout
  - filter state/query (+ digest)
  - pane focus set
  - pinned loops

## Crash-safe behavior

- Write path:
  - copy previous snapshot to `.bak`
  - write new snapshot to temp file + `sync_all`
  - atomic rename temp -> primary snapshot file
- Recovery path:
  - try primary snapshot first
  - if invalid/unreadable/digest mismatch, recover from `.bak`
  - return warnings for degraded recovery paths

## Implementation

- New module: `crates/forge-tui/src/crash_safe_state.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
