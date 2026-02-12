# TUI-905 plugin packaging, discovery, and lifecycle management

Task: `forge-axy`  
Status: delivered

## Scope

- Added plugin package format contracts (manifest/artifacts/signature).
- Added package encode/decode helpers.
- Added discovery/install/uninstall and runtime lifecycle state controls.
- Added signature verification against trusted signer registry.

## Package format

- `PluginPackage`
  - `schema_version`
  - `manifest`
  - `artifacts`
  - `signature`
- `PluginManifest`
  - id/name/version/description/entrypoint
  - required permissions
  - host API compatibility window (`min_host_api`, `max_host_api`)
- `PluginSignature`
  - signer
  - algorithm label
  - signature value

## Discovery and verification

- Discovery validates:
  - package schema version
  - manifest shape
  - signer trust
  - signature integrity
  - host compatibility window
- Signature model uses deterministic canonical payload hashing for reproducible checks.

## Lifecycle controls

- `ExtensionPackageManager` states:
  - `Discovered`
  - `Installed`
  - `Enabled`
  - `Running`
- Controls:
  - discover
  - install
  - enable/disable
  - start/stop runtime
  - uninstall
- Invalid state transitions return typed errors.

## Audit trail

- Every lifecycle transition emits a `PluginLifecycleEvent`:
  - plugin id
  - action
  - timestamp
  - detail

## Implementation

- New module: `crates/forge-tui/src/extension_package_manager.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
