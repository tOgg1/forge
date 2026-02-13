# TUI-909 layout snapshot breakpoint gate

Task: `forge-9r4`

What shipped:
- Added deterministic layout snapshot regression tests for core tabs:
- `Overview`, `Logs`, `Runs`, `MultiLogs`, `Inbox`
- Added viewport matrix coverage:
- `80x24`, `120x40`, `200x50`
- Added committed goldens:
- `crates/forge-tui/tests/golden/layout/*.txt` (15 files)

Implementation:
- New test harness: `crates/forge-tui/tests/layout_snapshot_test.rs`
- Fixture seeds realistic operator data (loops, run history, selected log, multi-log tails, inbox+claim events).
- Onboarding overlays dismissed per tab before snapshot capture to lock main-layout baselines.
- Snapshot writer supports local refresh:
- `UPDATE_GOLDENS=1 cargo test -p forge-tui --test layout_snapshot_test`

Why this matters:
- Catches layout regressions on cramped and wide terminals.
- Locks visual hierarchy for primary command-center panes.
- Provides deterministic baseline before deeper FrankenTUI visual polish passes.
