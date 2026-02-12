# TUI-906 reference plugins and extension developer docs

Task: `forge-exd`  
Status: delivered

## Scope

- Added a reference plugin bundle for extension developers.
- Added generated developer guide content from canonical reference plugin specs.
- Added permission safety lint warnings to reduce unsafe plugin patterns.
- Added regression tests for package discoverability + docs coverage.

## Reference bundle

- New module: `crates/forge-tui/src/extension_reference.rs`
- Core API:
  - `reference_plugin_specs`
  - `build_reference_plugin_bundle`
  - `permission_safety_warnings`
  - `render_extension_developer_guide`
- Reference plugins included:
  - `loop-health-inspector` (read-only diagnostics)
  - `safe-control-center` (constrained loop control actions)
  - `inbox-notes-assistant` (ticketed write-state breadcrumb updates)

## Safety posture

- Generated guidance includes an unsafe-pattern checklist:
  - avoid `execute-shell` where typed host actions exist
  - constrain `network-access` with allowlists/time budgets
  - treat `write-state` as privileged and auditable
- Permission lint helper emits warnings for elevated permissions.

## Implementation notes

- Reference packages use the `PluginPackage` contracts from `extension_package_manager`.
- Packages are signed using trusted signer flow (`sign_plugin_package`).
- Host compatibility windows are generated from provided host schema version.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
