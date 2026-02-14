# sv-8sf: Team task ingestion endpoints

Date: 2026-02-13
Task: `sv-8sf`

## Scope delivered

- Added CLI ingestion command:
  - `forge task send --team <id|name> --payload <json>`
  - persists task into team inbox (`team_tasks`)
- Added webhook ingestion handler primitives:
  - endpoint contract: `POST /teams/<id-or-name>/tasks`
  - bearer token auth check
  - basic per-team per-minute rate limit
  - response includes created `task_id`
- Added ingestion core:
  - `submit_team_task(...)`
  - webhook helpers + limiter state

## Files

- `crates/forge-cli/src/task.rs`
- `crates/forge-cli/src/lib.rs`

## Validation

```bash
cargo test -p forge-cli --lib task::tests:: -- --nocapture
cargo build -p forge-cli
```
