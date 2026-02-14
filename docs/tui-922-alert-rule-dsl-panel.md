# TUI-922 alert-rule DSL panel

Task: `forge-rhm`  
Status: delivered

## Scope

- Operator-defined alert rules for `status`, `log`, and `inbox` events.
- Deterministic compile diagnostics for invalid DSL lines.
- Rule evaluation output suitable for panel rendering and alert surfacing.

## Implementation

- Added module: `crates/forge-tui/src/alert_rule_dsl.rs`
- Added DSL compiler:
  - `compile_alert_rule_dsl` for line-based rule parsing.
  - Grammar:
    - `rule <id> when <source>.<field> <op> <value> [and ...] then <severity> "<message>"`
  - Sources: `status`, `log`, `inbox`
  - Operators: `==`, `!=`, `contains`, `!contains`, `>`, `>=`, `<`, `<=`
- Added event model + evaluator:
  - `AlertRuleEvent` (`Status|Log|Inbox`)
  - `evaluate_alert_rules` -> triggered alerts by matching source + predicates.
- Added panel helper:
  - `render_alert_rule_panel_lines` for compact panel/status output.
- Exported module in `crates/forge-tui/src/lib.rs`.

## Validation

- `cargo test -p forge-tui alert_rule_dsl::tests::`
- `cargo build -p forge-tui`
