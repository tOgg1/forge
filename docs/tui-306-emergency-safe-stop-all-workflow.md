# TUI-306 emergency safe-stop-all workflow

Task: `forge-yj4`  
Status: delivered

## Scope

- One-key emergency stop workflow model.
- Scope preview before execution.
- Staged stop progression (preview -> confirm -> request stop -> wait -> integrity checks).
- Post-stop integrity checks with escalation guidance.

## Implementation

- New module: `crates/forge-tui/src/emergency_safe_stop.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

### Included behavior

- `build_scope_preview(...)`
  - filter by project/pool/tags
  - deterministic selected/excluded/risk counts
  - summary string for operator confirmation
- `evaluate_emergency_safe_stop(...)`
  - requires explicit one-key confirmation (`Shift+X` equivalent)
  - computes staged execution status:
    - scope preview
    - hotkey confirm
    - stop request staging
    - await stopped
    - integrity checks
    - completion
  - adds escalation hints when blocked
- Integrity checks:
  - queue drained
  - ledger synced
  - runner health

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
