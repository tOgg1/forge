# Rust fmail-tui parity slices (2026-02-09)

## Landed slices

- `forge-w1f` live-tail parity:
  - `rust/crates/fmail-tui/src/live_tail.rs`
- `forge-rd8` notifications parity:
  - `rust/crates/fmail-tui/src/notifications.rs`
- `forge-jg2` timeline parity:
  - `rust/crates/fmail-tui/src/timeline.rs`
- `forge-fyx` state persistence + keymap/help parity:
  - `rust/crates/fmail-tui/src/state_help.rs`

All above validated with:
- `cd rust && cargo fmt --check`
- `cd rust && cargo clippy --workspace --all-targets -- -D warnings`
- `cd rust && cargo test --workspace`

## In-progress slices (local crate gates green; workspace drift blockers)

- `forge-egs` bookmarks parity:
  - `rust/crates/fmail-tui/src/bookmarks.rs`
- `forge-dz6` search parity:
  - `rust/crates/fmail-tui/src/search.rs`
- `forge-849` heatmap parity:
  - `rust/crates/fmail-tui/src/heatmap.rs`
- `forge-7a3` replay parity:
  - `rust/crates/fmail-tui/src/replay.rs`

Observed blocker pattern: concurrent workspace edits in `fmail-cli`/`fmail-core` repeatedly break full workspace gate even when `fmail-tui` crate gates pass.
