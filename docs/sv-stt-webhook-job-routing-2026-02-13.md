# sv-stt: Webhook job routing (2026-02-13)

Task: `sv-stt`
Epic: `sv-y0b`

## Delivered
- Implemented webhook request handling in `crates/forge-cli/src/webhook_server.rs`:
  - request gate: disabled `503`, non-POST `405`, unauthorized `401`
  - resolves webhook trigger by request path
  - handles `404` (no trigger) and `409` (ambiguous trigger path)
  - records job run via `JobStore::record_run`
  - returns JSON response with `run_id`, `job_name`, `status`
- Webhook auth integrated via `crates/forge-cli/src/webhook_auth.rs`.

## Acceptance mapping
- Webhook triggers job and returns run id:
  - covered by `webhook_server::tests::routed_webhook_records_job_run_and_returns_run_id`

## Validation
- `cargo fmt --package forge-cli`
- `cargo test -p forge-cli webhook_server::tests:: -- --nocapture`
- `cargo build -p forge-cli`
