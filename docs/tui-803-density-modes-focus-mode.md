# TUI-803 density modes and focus mode

Task: `forge-d8z`  
Status: delivered

## Scope

- Added dual-density support: `comfortable` and `compact`.
- Added deep focus mode for distraction-minimized debugging.
- Wired keyboard controls and command-palette actions for both features.
- Applied density/focus behavior to multi-log matrix layout calculations.

## Implementation

- Updated model/rendering in `crates/forge-tui/src/app.rs`:
  - new enums: `DensityMode`, `FocusMode`
  - controls:
    - `M` cycles density mode
    - `Z` toggles deep focus mode
    - `z` keeps zen split/right-pane toggle behavior
  - header/footer now expose active density + focus state
  - deep focus hides tab bar and uses minimal footer hints
- Updated multi-log matrix rendering in `crates/forge-tui/src/multi_logs.rs`:
  - dynamic header rows, gaps, and min cell dimensions based on density/focus
  - compact mode packs more panes per page when terminal size permits
- Updated command palette in `crates/forge-tui/src/command_palette.rs`:
  - `Cycle Density Mode`
  - `Toggle Deep Focus Mode`

## Regression tests

- Added app tests for:
  - density mode cycling (`M`)
  - deep focus toggle behavior (`Z`)
  - compact-density page-capacity behavior for multi-log matrix
- Updated palette registry stability test for new typed actions.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
