# TUI-920 budget guardrails

Task: `forge-gpb`  
Status: delivered

## Scope

- Budget guardrails model for loop/cluster/fleet scopes.
- Threshold states/actions:
  - warn at 80%
  - pause at 95%
  - hard-kill at 100%
- Burn-rate and projected exhaustion ETA.
- Efficiency metrics:
  - cost per task completed
  - tokens per task completed
- One-key budget extension helper.
- JSON persistence/restore for restart carry-over.

## Implementation

- New module: `crates/forge-tui/src/budget_guardrails.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

### Core API

- `BudgetScope` / `BudgetScopeKind`: loop, cluster, fleet budget keys.
- `BudgetPolicy`: token/cost limits + threshold ratios + auto pause/kill controls.
- `BudgetLedgerStore` + `BudgetLedgerEntry`: persisted guardrail state.
- `evaluate_budget_guardrails(...)`:
  - computes utilization ratios and guardrail state/action
  - computes burn rates and exhaustion projection
  - computes per-task efficiency metrics
- `apply_one_key_budget_extension(...)`: ratio/min-based budget bump.
- `persist_budget_ledger(...)` / `restore_budget_ledger(...)`: schema-backed JSON state.

## Validation

- `cargo fmt --all`
- `cargo test -p forge-tui budget_guardrails::`
- `cargo build -p forge-tui`
