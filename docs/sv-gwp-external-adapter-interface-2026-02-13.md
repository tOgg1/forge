# sv-gwp: External adapter interface (Linear/Slack)

Date: 2026-02-13
Task: `sv-gwp`

## Scope delivered

- Added adapter interface in `forge-cli`:
  - `ExternalAdapter` trait (`kind`, `enabled`, `poll_tasks`)
  - shared payload model `ExternalTask`
  - ingest results model `AdapterIngestResult`
- Added stub adapters:
  - `LinearStubAdapter`
  - `SlackStubAdapter`
- Added config wiring helper:
  - `AdapterRuntimeConfig` with map-based enable flags
  - `build_stub_adapters(...)` creates adapters with enabled/disabled state
- Added team inbox ingest path:
  - `ingest_adapter_tasks(...)` maps external payloads to team task payload JSON and submits to `team_tasks`
  - `ingest_enabled_adapters(...)` runs all adapters and returns per-adapter results
- Disabled adapters return no-op results and do not write inbox tasks.

## Files

- `crates/forge-cli/src/external_adapter.rs`
- `crates/forge-cli/src/lib.rs`

## Validation

```bash
cargo test -p forge-cli --lib external_adapter::tests:: -- --nocapture
cargo build -p forge-cli
```
