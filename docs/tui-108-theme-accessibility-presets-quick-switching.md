# TUI accessibility presets and quick switching

Task: `forge-5m9`  
Status: delivered

## Scope

- Added explicit accessibility presets:
  - `high-contrast`
  - `low-light`
  - `colorblind-safe`
- Kept existing curated themes (`default`, `ocean`, `sunset`).
- Added quick accessibility switch keypath (`T`) while keeping full theme cycle (`t`).

## Implementation

- Theme/preset model updates in `crates/forge-tui/src/theme.rs`:
  - Added new curated palettes and semantic-slot mappings.
  - Added `ACCESSIBILITY_PRESET_ORDER`.
  - Added `cycle_accessibility_preset` helper.
- App integration in `crates/forge-tui/src/app.rs`:
  - Added `cycle_accessibility_theme`.
  - Wired `T` in main and expanded-logs modes.
  - Updated help/footer guidance text.
- Keymap/help parity:
  - `crates/forge-tui/src/keymap.rs`
  - `crates/forge-tui/src/help_overlay.rs`
- Config parity:
  - Added new valid theme values in `crates/forge-core/src/config.rs`.
  - Updated sample config comment in `crates/forge-cli/src/config.rs`.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
