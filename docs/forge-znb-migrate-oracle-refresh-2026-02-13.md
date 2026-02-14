# forge-znb - migrate oracle fixture refresh (2026-02-13)

## Summary

`internal/cli` migrate oracle fixture drifted after schema migrations 14/15 (team model/tasks).
Fixture expected latest at version 13 and stale migration counts.

## Changes

- Regenerated oracle fixture:
  - `old/go/internal/cli/testdata/oracle/migrate.json`

Updated expectations now include:

- `migrate status` entries for versions 14 and 15
- `migrate up (apply pending)` count updated to 14
- `migrate up --to (latest)` target updated to version 15

## Validation

```bash
cd old/go
FORGE_UPDATE_GOLDENS=1 env -u GOROOT -u GOTOOLDIR go test ./internal/cli -run '^TestMigrateOracleScenarioMatchesFixture$' -count=1
env -u GOROOT -u GOTOOLDIR go test ./internal/cli -run '^TestMigrateOracleScenarioMatchesFixture$' -count=1
env -u GOROOT -u GOTOOLDIR go test ./internal/cli -count=1
```

All pass.
