# sv-crq: Webhook auth

Date: 2026-02-13
Task: `sv-crq`

## Scope delivered

- Bearer-token webhook auth module in `crates/forge-cli/src/webhook_auth.rs`:
  - `WebhookAuthConfig` with env/config-map loading
  - `authorize_webhook_request(...)` and decision envelope
  - case-insensitive `Authorization` header handling
  - bearer format validation and `401` rejection reasons
- Webhook gate integration in `crates/forge-cli/src/webhook_server.rs` via `validate_request_gate(...)`.

## Acceptance checks

- Unauthorized requests return `401`.
- Correct bearer token authorizes request.

## Validation

```bash
cargo test -p forge-cli --lib webhook_auth::tests:: -- --nocapture
cargo test -p forge-cli --lib webhook_server::tests:: -- --nocapture
cargo check -p forge-cli
```
