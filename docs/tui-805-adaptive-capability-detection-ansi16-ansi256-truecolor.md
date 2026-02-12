# TUI-805 adaptive capability detection (ANSI16/256/truecolor)

Task: `forge-bjj`  
Status: delivered

## Scope

- Added runtime terminal capability detection for color support tiers.
- Added capability-aware palette fallback behavior to preserve readability.
- Added capability-aware adapter theme mapping for style token selection.
- Wired runtime detection into the `forge-tui` binary app construction path.

## Implementation

- Updated module: `crates/forge-tui/src/theme.rs`
  - added `TerminalColorCapability::from_slug`
  - added `detect_terminal_color_capability`
  - added `resolve_palette_for_capability`
  - detection sources:
    - override: `FORGE_TUI_COLOR_CAPABILITY`
    - standard hints: `TERM`, `COLORTERM`, `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR_FORCE`
  - policy:
    - `ansi16` forces `high-contrast` palette for readability
    - `ansi256` and `truecolor` keep requested palette
- Updated module: `crates/forge-tui/src/lib.rs`
  - added `theme_for_capability`
  - added `detected_theme`
  - maps `ansi16` to adapter `ThemeKind::HighContrast`
- Updated module: `crates/forge-tui/src/app.rs`
  - added `App::new_with_capability`
  - `App::new` now delegates to `new_with_capability(..., TrueColor, ...)` to keep existing tests stable
  - stores capability and renders with capability-derived theme tokens
- Updated binary entrypoint: `crates/forge-tui/src/bin/forge-tui.rs`
  - runtime capability detection now used when constructing the app

## Regression tests

- Added `theme.rs` tests for:
  - `COLORTERM=truecolor` detection
  - `TERM=*256color` detection
  - `NO_COLOR` handling
  - `FORCE_COLOR` override behavior
  - ANSI16 high-contrast palette fallback
- Added `app.rs` test for ANSI16 constructor behavior.
- Added `lib.rs` test for ANSI16 -> high-contrast theme mapping.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
