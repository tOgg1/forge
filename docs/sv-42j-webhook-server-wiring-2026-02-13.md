# sv-42j: Webhook server wiring

Date: 2026-02-13
Task: `sv-42j`

## Scope delivered

- Added webhook request handler wiring in `crates/forge-cli/src/webhook_server.rs`:
  - `handle_webhook_request(config, store, request)`
  - request gate reuse (`enabled`, `POST` only, bearer auth)
  - webhook path normalization (query-stripped)
- Wired endpoint routing against persisted webhook triggers (`trigger_type == "webhook"`).
- Added response behavior:
  - `200` with `{run_id, job_name, status}` when a route is matched and run is recorded
  - `404` when no webhook trigger maps to path
  - `409` when multiple webhook triggers map to one path
  - `500` on persistence/runtime failures

## Validation

```bash
cargo test -p forge-cli --lib webhook_server::tests:: -- --nocapture
cargo check -p forge-cli
```
