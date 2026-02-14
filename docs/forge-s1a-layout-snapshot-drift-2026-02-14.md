# forge-s1a - forge-tui layout snapshot drift

Date: 2026-02-14
Task: `forge-s1a`
Scope:
- `crates/forge-tui/tests/golden/layout/inbox_80x24.txt`
- `crates/forge-tui/tests/golden/layout/inbox_120x40.txt`
- `crates/forge-tui/tests/golden/layout/inbox_200x50.txt`
- `crates/forge-tui/tests/golden/layout/multi_logs_80x24.txt`
- `crates/forge-tui/tests/golden/layout/overview_120x40.txt`
- `crates/forge-tui/tests/golden/layout/overview_200x50.txt`

## Change

- Workspace test sweep failed in `layout_snapshot_test` due stale golden snapshots.
- Regenerated layout goldens for the test matrix to match current rendering behavior.
- Inbox snapshots now include the markdown detail block in the right pane.

## Validation

```bash
cargo test --workspace --all-targets
UPDATE_GOLDENS=1 cargo test -p forge-tui --test layout_snapshot_test
cargo test -p forge-tui --test layout_snapshot_test
```

Result:
- Initial workspace sweep identified the failing layout snapshot case.
- Regenerated layout snapshots.
- `layout_snapshot_test` now passes without `UPDATE_GOLDENS`.
