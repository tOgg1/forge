# TUI-901 extension API for custom panels

Task: `forge-cpt`  
Status: delivered

## Scope

- Added stable extension API for custom TUI panels.
- Supports both panel classes:
  - read-only panels
  - interactive panels
- Defines deterministic lifecycle + render contracts for host/runtime integration.

## Contracts

- Descriptor contract (`ExtensionPanelDescriptor`):
  - stable panel id/title/version/mode/description
- Lifecycle contract (`ExtensionPanel` trait):
  - `on_mount`
  - `on_event`
  - `render`
  - `on_unmount`
- Session state contract (`PanelSessionState`):
  - `Created`
  - `Mounted`
  - `Closed`

## Host/runtime API

- `PanelRegistry`
  - register panel descriptors + factories
  - deterministic descriptor listing
  - create panel sessions by id
- `PanelSession`
  - mount / dispatch events / render / unmount
  - lifecycle enforcement with typed errors
  - read-only guard: input events are ignored with status effect

## Event and effect model

- `PanelEvent`
  - `Input`
  - `Tick`
  - `DataRefresh`
- `PanelUpdate`
  - effect list (`PanelEffect`)
  - close request signal

## Implementation

- New module: `crates/forge-tui/src/extension_api.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
