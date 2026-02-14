# TUI Next-Action Panel (Overview)

Task: `forge-1wq`  
Date: `2026-02-13`  
Status: delivered

## Scope

- Verify Overview Next-Action panel is active in app render path.
- Verify rules-based suggestions + jump shortcuts render in panel.
- Verify breakpoint layout snapshots include Next-Action panel where space allows.

## Implementation anchors

- App overview render opts into Next-Action slot:
  - `crates/forge-tui/src/app.rs:4281`
  - `crates/forge-tui/src/app.rs:4295`
- Next-Action rule builder:
  - `crates/forge-tui/src/overview_tab.rs:32`
- Next-Action panel rendering:
  - `crates/forge-tui/src/overview_tab.rs:575`
  - `crates/forge-tui/src/overview_tab.rs:578`
- Regression tests:
  - `crates/forge-tui/src/overview_tab.rs:987`
  - `crates/forge-tui/src/overview_tab.rs:1037`
  - `crates/forge-tui/src/overview_tab.rs:1090`
- Breakpoint golden snapshots:
  - `crates/forge-tui/tests/layout_snapshot_test.rs:285`
  - `crates/forge-tui/tests/golden/layout/overview_120x40.txt`
  - `crates/forge-tui/tests/golden/layout/overview_200x50.txt`

## Validation

- `cargo test -p forge-tui --lib paneled_overview_next_action_slot_ -- --nocapture`
- `cargo test -p forge-tui --test layout_snapshot_test key_layout_snapshots_across_breakpoints -- --nocapture`
- `cargo build -p forge-tui`
