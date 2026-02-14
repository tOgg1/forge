# TUI-79B session recording and replay core

## Scope
- Add deterministic recording/replay primitives for shareable TUI timelines.

## Changes
- Added `crates/forge-tui/src/session_recording.rs`:
  - recording model (`SessionRecording`, `RecordedFrame`, `RecordedInput`)
  - append helpers with monotonic timestamp validation
  - compaction helper (`compact_recording`) for duplicate consecutive frames
  - replay snapshot resolver (`replay_snapshot_at`)
  - replay text renderer (`render_replay_lines`)
- Exported module via `crates/forge-tui/src/lib.rs`.

## Validation
- `cargo test -p forge-tui session_recording::tests:: -- --nocapture`
- `cargo build -p forge-tui`
