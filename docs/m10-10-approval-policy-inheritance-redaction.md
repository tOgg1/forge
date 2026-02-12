# M10.10 approval-policy inheritance and redaction

Task: `forge-26e`
Status: delivered

## Scope delivered

- `forge agent run` now accepts:
  - `--approval-policy`
  - `--account-id`
  - `--profile`
- Parent context inheritance order for child spawn defaults:
  1. explicit run flags
  2. persistent-agent labels (when available)
  3. parent env (`FORGE_APPROVAL_POLICY`, `FORGE_ACCOUNT_ID`, `FORGE_PROFILE`)
  4. fallback approval policy: `strict`
- Child spawn now forwards inherited context in spawn env and seeds persistent-agent labels on create.
- `forge agent send` and `forge agent interrupt` now enforce strict-policy guardrails for risky actions, with explicit override via `--allow-risky`.
- Persistent-agent event writes now pass through redaction:
  - sensitive keys masked (token/secret/password/api_key/etc)
  - bearer/token-like payload strings masked
  - applied to metrics, revive audit events, gc events, and summary snapshot event payloads.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
