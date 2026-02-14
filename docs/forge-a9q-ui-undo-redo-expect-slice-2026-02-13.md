# forge-a9q - forge-tui ui_undo_redo expect-used slice

Date: 2026-02-13
Task: `forge-a9q`
Scope: `crates/forge-tui/src/ui_undo_redo.rs`

## Change

- Replaced all test `expect(...)` callsites with explicit `match` + panic context in:
  - `undo_restores_prior_selection_scroll_filter_snapshot`
  - `redo_restores_snapshot_after_undo`
  - `checkpoint_after_undo_clears_redo_stack`
  - `max_history_evicts_oldest_snapshots`

## Validation

```bash
cargo test -p forge-tui --lib ui_undo_redo::tests
rg -n "expect\\(" crates/forge-tui/src/ui_undo_redo.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'ui_undo_redo.rs' || true
```

Result:
- `ui_undo_redo` tests passed (`5 passed`).
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.

