# TUI-704 incident replay mode and timeline reconstruction

Task: `forge-h18`  
Status: delivered

## Scope

- Added deterministic incident replay model from recorded event streams.
- Added timeline reconstruction with replay hotspot detection for postmortem triage.
- Added time controls for scrub, event step, and accelerated playback.
- Added annotation timeline support in replay snapshots.

## Replay behavior

- `ReplayControls` supports:
  - time range normalization
  - ratio-based cursor seek
  - previous/next event stepping
  - playback advance with speed multipliers (`1x/5x/10x/30x`) and end-stop auto pause
- Snapshot generation keeps replay cursor/state consistent with event time bounds.
- Duplicate event ids are dropped deterministically and surfaced via `dropped_duplicate_events`.

## Timeline reconstruction

- Converts replay events into timeline buckets via existing scrubber heatmap primitive.
- Flags replay hotspots from error-heavy or dense windows.
- Exposes:
  - reconstructed `TimelineHeatmap`
  - ranked hotspot list (`ReplayHotspot`)
  - replay-visible events and annotations at cursor time

## Implementation

- New module: `crates/forge-tui/src/incident_replay.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Regression tests

- replay range derivation from event stream
- seek/step/advance time-control semantics
- dedupe + visibility filtering in replay snapshot
- timeline reconstruction + hotspot detection behavior
- empty-input replay snapshot fallback behavior

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
