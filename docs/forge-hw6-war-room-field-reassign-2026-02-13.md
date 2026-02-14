# forge-hw6: war_room_mode field_reassign_with_default slice (2026-02-13)

## Scope
Fix clippy `field_reassign_with_default` in `crates/forge-tui/src/war_room_mode.rs` tests.

## Changes
Updated three tests to initialize `WarRoomState` via struct update syntax instead of mutating fields after `Default::default()`:

- `follow_leader_sync_uses_leader_view`
- `consensus_sync_uses_majority_tab_and_average_scroll`
- `render_lines_include_header_and_participants`

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib follow_leader_sync_uses_leader_view
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::iter_nth_zero -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused war-room test passed
- clippy run still fails elsewhere, but `war_room_mode.rs` no longer appears in diagnostics
