# sv-4tb - Team task inbox + assignment lifecycle (2026-02-13)

## Scope shipped
- Added migration `015_team_tasks`:
  - `team_tasks` table:
    - task payload JSON
    - status (`queued|assigned|running|blocked|done|failed|canceled`)
    - explicit `priority`
    - assignment + lifecycle timestamps
  - `team_task_events` append-only audit log:
    - event type
    - from/to status
    - actor + detail
  - indexes + update trigger + rollback SQL
- Added repository/service module:
  - `crates/forge-db/src/team_task_repository.rs`
  - models:
    - `TeamTask`
    - `TeamTaskEvent`
    - `TeamTaskStatus`
    - `TeamTaskFilter`
  - repository APIs:
    - submit/get/list
    - assign/reassign/start/block/complete/fail/cancel
    - list_events
  - service APIs:
    - submit/list_queue
    - assign/reassign
    - complete/fail
- Payload validation on submit:
  - JSON object required
  - required string fields: `type`, `title`
- Deterministic queue ordering:
  - `priority ASC`, then `submitted_at ASC`
- Transition guardrails:
  - rejects invalid transitions (especially from terminal statuses)
- Added DB errors:
  - `TeamTaskNotFound`
  - `TeamTaskAlreadyExists`

## Wiring
- Exported module from `crates/forge-db/src/lib.rs`.

## Tests added
- `crates/forge-db/tests/team_task_repository_test.rs`
  - submit/list ordering
  - payload schema validation
  - assign/reassign/complete + audit events
  - fail path + status/assignee filtering
  - persistence across DB reopen
- `crates/forge-db/tests/migration_015_test.rs`
  - embedded SQL parity with Go migration files
  - up/down schema parity checks

## Validation
```bash
cargo fmt --package forge-db
cargo test -p forge-db --test team_task_repository_test -- --nocapture
cargo test -p forge-db --test migration_015_test -- --nocapture
cargo test -p forge-db --test team_repository_test -- --nocapture
cargo test -p forge-db --test migration_014_test -- --nocapture
```
