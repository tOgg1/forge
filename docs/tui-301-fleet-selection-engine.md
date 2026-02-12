# TUI-301 fleet selection engine with expressive filters

Task: `forge-exn`
Status: delivered

## Scope

- Add fleet selection primitives for:
  - loop id prefix
  - name
  - repo
  - profile
  - pool
  - state
  - tag
  - stale status
- Add action preview output before execution.

## Implementation

- New module: `crates/forge-tui/src/fleet_selection.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Selection model

- `FleetLoopRecord` is canonical loop row used for selection.
- `FleetSelectionFilter` supports combinable filters:
  - `id_prefix`, `name_contains`, `repo_contains`
  - `profile`, `pool`
  - `states` (multi-value)
  - `required_tags` (all-of matching)
  - `stale` (`Some(true|false)` or no stale filter)
- Matching is case-insensitive and trim-normalized.

## Preview model

- `preview_fleet_action` builds pre-execution target preview:
  - selected count
  - deterministic target id sample
  - summary text
  - command preview string
- Preview truncates displayed targets with explicit `(+N more)` suffix.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
