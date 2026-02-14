# sv-fky - Team model + persistence (2026-02-13)

## Scope shipped
- Added migration `014_team_model`:
  - `teams` table with config fields:
    - `delegation_rules_json`
    - `default_assignee`
    - `heartbeat_interval_seconds`
  - `team_members` table with role (`leader` | `member`) and unique membership per team/agent.
  - indexes + update timestamp trigger + rollback SQL.
- Added DB repository/service module:
  - `crates/forge-db/src/team_repository.rs`
  - model types: `Team`, `TeamMember`, `TeamRole`
  - persistence APIs:
    - team create/get/get_by_name/list/update/delete
    - member add/remove/list
  - service APIs:
    - create/list/show/delete team
    - add/list members
  - validation on write:
    - team name required
    - heartbeat interval must be > 0
    - delegation rules must be valid JSON object
    - role must be `leader` or `member`

## Wiring
- Exported `team_repository` from `crates/forge-db/src/lib.rs`.
- Added DB error variants:
  - `TeamNotFound`
  - `TeamAlreadyExists`
  - `TeamMemberNotFound`
  - `TeamMemberAlreadyExists`

## Tests added
- `crates/forge-db/tests/team_repository_test.rs`
  - create/list/show/delete roundtrip
  - config validation failure paths
  - member role storage/listing + duplicate handling
  - update validation/persistence
- `crates/forge-db/tests/migration_014_test.rs`
  - embedded SQL parity with Go migration files
  - up/down migration parity for schema objects + constraints

## Validation
```bash
cargo fmt --package forge-db
cargo test -p forge-db --test team_repository_test -- --nocapture
cargo test -p forge-db --test migration_014_test -- --nocapture
```
