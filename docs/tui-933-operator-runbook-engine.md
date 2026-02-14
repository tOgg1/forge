# TUI-933: Operator Runbook Engine

Task: `forge-agq`

## Scope
- Define reusable guided operator runbooks.
- Support lightweight TOML/YAML runbook manifests.
- Provide execution state transitions and compact panel rendering.
- Include built-ins for key operations: shift handoff, incident response, fleet scale-up, graceful shutdown.

## Implementation
- Added `crates/forge-tui/src/operator_runbook_engine.rs`.
- Data model:
  - `RunbookDefinition`, `RunbookStep`
  - `RunbookRun`, `RunbookRunStep`, `RunbookStepStatus`, `RunbookStepAction`
- Built-ins:
  - `shift-handoff`
  - `incident-response`
  - `fleet-scale-up`
  - `graceful-shutdown`
- Engine:
  - `start_runbook(...)`
  - `apply_runbook_step_action(...)`
  - `next_recommended_step_index(...)`
  - `render_runbook_lines(...)`
- Manifest parsing:
  - `parse_runbook_manifest(...)`
  - TOML-like parser for `id/title/description` + `[[steps]]`
  - YAML-like parser for `id/title/description` + `steps:` list
- Exported module via `crates/forge-tui/src/lib.rs`.

## Regression Tests
- `operator_runbook_engine::tests::parse_toml_manifest`
- `operator_runbook_engine::tests::parse_yaml_manifest`
- `operator_runbook_engine::tests::builtin_runbooks_include_required_operator_flows`
- `operator_runbook_engine::tests::runbook_progression_moves_to_next_pending_step`
- `operator_runbook_engine::tests::render_runbook_lines_contains_progress_and_step_details`

## Validation
- `cargo fmt --package forge-tui`
- `cargo test -p forge-tui operator_runbook_engine::tests:: -- --nocapture`
- `cargo build -p forge-tui`

## Notes
- While validating, `crates/forge-tui/src/lib.rs` had a duplicate `pub mod what_if_simulator;` line that blocked compilation; removed duplicate as a minimal compile-unblock fix.
