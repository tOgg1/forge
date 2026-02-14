# TUI Overview Fourth-Slot Seam (2026-02-13)

Task: `forge-27c`  
Scope: reserve optional overview panel slot for upcoming next-action panel.

## What shipped

- Added `OverviewPaneOptions` seam in `crates/forge-tui/src/overview_tab.rs`.
- Added `render_overview_paneled_with_options(...)`.
- Default path remains hidden (`reserve_next_action_slot=false`).
- Optional slot renders a reserved panel (`Next Action (reserved)`) when enabled.

## Validation

- `cargo test -p forge-tui --lib paneled_overview_next_action_slot_ -- --nocapture`
- `cargo test -p forge-tui --lib paneled_overview_shows_work_domains_when_space_allows -- --nocapture`
- `cargo build -p forge-tui`
