# TUI-802 accessibility contrast validator

Task: `forge-zzw`  
Status: delivered

## Scope

- Added automated contrast validation for theme packs.
- Added terminal capability-aware checks (`truecolor`, `ansi256`, `ansi16`).
- Added fail-fast validation mode returning first actionable violation.
- Added full report mode with per-theme/per-slot violation details.

## Implementation

- Updated module: `crates/forge-tui/src/theme.rs`
- New contracts:
  - `TerminalColorCapability`
  - `ContrastViolation`
  - `ContrastValidationReport`
- New APIs:
  - `validate_theme_packs_contrast`
  - `validate_theme_packs_contrast_fail_fast`
  - `validate_curated_theme_contrast`
  - `validate_curated_theme_contrast_fail_fast`

## Validation logic

- Checks semantic slot contrast pairs against minimum thresholds.
- Applies capability-specific color quantization before ratio checks:
  - ANSI 256 quantization
  - ANSI 16 nearest-color quantization
- Computes WCAG-style relative luminance contrast ratio.
- Supports fail-fast and aggregate reporting modes.

## Regression tests

- Curated packs pass across all capabilities.
- Bad pack reports violations in aggregate mode.
- Fail-fast mode returns first violation with slot context.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
