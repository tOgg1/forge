# TUI-MRY war room mode (synchronized views)

## Scope
- Add synchronized-view core state for war room operations.

## Changes
- Added `crates/forge-tui/src/war_room_mode.rs`:
  - participant model (`WarRoomParticipant`)
  - sync modes (`Off`, `FollowLeader`, `Consensus`)
  - shared-view state (`SharedViewState`, `WarRoomState`)
  - reconciliation logic (`reconcile_shared_view`)
  - stale participant detection (`stale_participants`)
  - compact text renderer (`render_war_room_lines`)
- Exported module in `crates/forge-tui/src/lib.rs`.

## Validation
- `cargo test -p forge-tui war_room_mode::tests:: -- --nocapture`
- `cargo build -p forge-tui`
