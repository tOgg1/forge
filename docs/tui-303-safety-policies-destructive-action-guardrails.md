# TUI-303 safety policies and destructive-action guardrails

Task: `forge-5bh`  
Status: delivered

## Scope

- Add policy-aware blocking for risky loop actions.
- Require explicit confirmation flow for destructive actions.
- Provide escalation hints when policy blocks execution.
- Emit override audit entries when policy exceptions are used.

## Implementation

- Extended action model in `crates/forge-tui/src/actions.rs`:
  - `GuardrailPolicy`
  - `ActionTarget`
  - `PolicyOverride`
  - `OverrideAuditEntry`
  - `GuardrailDecision`
  - `evaluate_action_guardrail(...)`
- Guardrail rules cover:
  - protected pools
  - protected tags
  - batch-size threshold for destructive actions
- Escalation guidance added for blocked outcomes.
- Confirm flow remains required for `stop|kill|delete` via existing confirm helpers.
- Override flow now records a structured audit entry with actor/reason/ticket/approver/timestamp/rule.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
