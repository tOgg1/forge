# TUI-104 layout preset system (ops/dev/review/night)

Task: `forge-bx4`  
Status: delivered

## Scope

- Added persisted layout preset model for multi-pane TUI views.
- Added built-in presets:
  - `ops`
  - `dev`
  - `review`
  - `night`
- Added schema versioning with migration handling.
- Added corruption-resilient restore behavior.

## Model

- `LayoutPreset`
  - `id`, `label`
  - `rows`, `cols`
  - `multi_page`
  - `focus_right`
  - `pinned_first`
- `LayoutPresetStore`
  - `schema_version`
  - `active_preset_id`
  - `presets`
- `LayoutPresetLoadOutcome`
  - `store`
  - `migrated`
  - `warnings`

## Persistence and restore

- `persist_layout_preset_store` serializes normalized store to JSON.
- `restore_layout_preset_store` restores from JSON with resilience:
  - invalid JSON -> defaults + warning
  - malformed entries ignored
  - duplicate ids deduped
  - missing active preset falls back to `ops`
  - missing built-ins are re-added

## Schema and migration

- Current schema: `2` (`LAYOUT_PRESET_SCHEMA_VERSION`).
- Supports migration from v1 shape:
  - `active` -> `active_preset_id`
  - supports v1 `grid: "RxC"` parsing
  - fills v2 fields (`multi_page`, `pinned_first`) with safe defaults

## Layout application

- `apply_layout_preset` computes effective layout via `fit_pane_layout`:
  - preserves requested preset intent
  - degrades safely on constrained viewport sizes

## Implementation

- New module: `crates/forge-tui/src/layout_presets.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
