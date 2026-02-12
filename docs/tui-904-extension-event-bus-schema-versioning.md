# TUI-904 event bus and schema versioning for plugins

Task: `forge-7cn`  
Status: delivered

## Scope

- Added internal plugin event bus with versioned event envelopes.
- Added per-plugin schema compatibility declarations.
- Added compatibility enforcement rules to keep plugins stable across releases.

## Event model

- `PluginEventEnvelope`
  - event id
  - kind
  - schema version (`major.minor`)
  - emitted timestamp
  - payload map
- `PluginEventKind`
  - loop selection changes
  - tab changes
  - palette action execution
  - panel lifecycle
  - sandbox decisions

## Compatibility rules

- Plugins register:
  - subscribed event kinds
  - schema compatibility windows per kind (`major`, `min_minor`, `max_minor`)
- Event delivery requires:
  - subscription exists
  - matching major version
  - event minor within declared plugin range
- Out-of-range or undeclared compatibility is skipped with explicit reason.

## Bus behavior

- `ExtensionEventBus`
  - register subscribers
  - set schema versions per event kind
  - publish events with dispatch report
  - drain per-plugin inbox queues
- `DispatchReport`
  - delivered plugin ids
  - skipped plugin ids + reason

## Implementation

- New module: `crates/forge-tui/src/extension_event_bus.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
