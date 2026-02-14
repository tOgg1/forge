# TUI-923 event hook automation

Task: `forge-rrr`  
Status: delivered

## Scope

- Run configured scripts/commands when selected rule-driven events fire.
- Enforce guardrails for repeated triggers (cooldown + hourly caps).
- Provide deterministic panel-ready run planning output.

## Implementation

- Added module: `crates/forge-tui/src/event_hook_automation.rs`
- Added hook config/state/plan model:
  - `EventHookConfig`, `HookAutomationState`, `HookExecutionPlan`
  - `PlannedHookRun`, `SkippedHookRun`, `HookExecutionRecord`
- Added planner:
  - `plan_event_hook_runs(configs, triggered_alerts, now, state)`
  - Trigger source is `alert_rule_dsl::TriggeredRuleAlert`
  - Matching: configured `rule_ids`
  - Guardrails:
    - per-hook cooldown (`cooldown_s`)
    - per-hook hourly cap (`max_runs_per_hour`)
    - disabled + missing command checks
- Added state update helper:
  - `record_hook_execution` with bounded history retention
- Added panel helper:
  - `render_event_hook_panel_lines`
- Exported module in `crates/forge-tui/src/lib.rs`.

## Validation

- `cargo test -p forge-tui event_hook_automation::tests::`
- `cargo build -p forge-tui`
