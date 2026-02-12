# TUI-801 theme packs with semantic color slots

Task: `forge-9hq`  
Status: delivered

## Scope

- Added curated theme pack model with semantic color slots.
- Added status/token semantic slot mapping for every curated palette.
- Added theme pack import/export support with schema validation.
- Added regression tests for slot completeness and import/export round-trip.

## Implementation

- Updated module: `crates/forge-tui/src/theme.rs`
- Added theme-pack contracts:
  - `ThemeSemanticSlot`
  - `ThemePack`
  - `ThemePackError`
- Added APIs:
  - `curated_theme_packs`
  - `resolve_theme_pack`
  - `cycle_theme_pack`
  - `export_theme_pack`
  - `import_theme_pack`
- Existing palette APIs remain compatible:
  - `resolve_palette`
  - `cycle_palette`

## Semantic slot coverage

- UI slots: background, surfaces, text, border, accent, focus.
- Status slots: success, warning, error, info.
- Token-class slots: keyword, string, number, command, path.

## Import/export

- Export format: deterministic JSON with `schema_version`, `id`, `title`, `palette_name`, `slots`.
- Import validation enforces:
  - schema version compatibility
  - normalized id/title/palette fields
  - known semantic slot keys only
  - required-slot completeness
  - strict hex-color format (`#RRGGBB`)

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
