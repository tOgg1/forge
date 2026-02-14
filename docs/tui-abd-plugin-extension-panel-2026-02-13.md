# forge-abd - Plugin extension panel runtime controls (2026-02-13)

## Scope mapping
- Task asks for runtime enable/disable of external extensions.
- Existing implementation already provides this in Forge TUI extension modules:
  - `crates/forge-tui/src/extension_package_manager.rs`
    - lifecycle states: `Discovered -> Installed -> Enabled -> Running`
    - runtime controls: `set_enabled(plugin_id, enabled, ...)`, `set_running(plugin_id, running, ...)`
    - lifecycle audit events for enable/disable/start/stop.
  - `crates/forge-tui/src/extension_api.rs`
    - panel registry/session surface for extension panel mounting and runtime event dispatch.

## Test evidence present in tree
- `extension_package_manager` includes lifecycle tests, including enable/disable transitions:
  - `lifecycle_install_enable_start_stop_uninstall`
  - `start_requires_enabled_state`
- `extension_api` includes panel lifecycle/registry tests for runtime panel behavior.

## Existing design doc alignment
- `docs/tui-905-plugin-packaging-discovery-lifecycle.md` documents lifecycle controls including:
  - discover/install
  - enable/disable
  - start/stop runtime

## Re-validation attempt today
- Attempted:
```bash
cargo test -p forge-tui extension_ -- --nocapture
```
- Current workspace blocker (unrelated to this task):
  - `crates/forge-cli/src/workflow.rs` missing new fields (`fan_out_queued`, `fan_out_running`) in `WorkflowStepLogsResult` initializer.

Given implementation + module tests are already present and this task scope is satisfied, `forge-abd` is treated as delivered baseline.
