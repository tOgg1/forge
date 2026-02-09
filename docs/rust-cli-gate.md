# Rust CLI Parity Gate

Task: `forge-pfe`  
Status: in-progress

## Goal

Define strict, testable criteria for Forge CLI parity before Rust cutover.

## Scope

- Go source contracts:
  - `cmd/forge`
  - `internal/cli`
- Rust target crate:
  - `forge-cli`

## Required gate criteria

1. Help and command surface parity
- Root help snapshot must match oracle baseline:
  - `internal/parity/testdata/oracle/expected/forge/root/help.txt`
- Global flags snapshot must match oracle baseline:
  - `internal/parity/testdata/oracle/expected/forge/root/global-flags.txt`
- Drift test baseline: `TestCLIGateRootOracleBaseline`.

2. Error envelope parity
- Invalid-flag behavior must match oracle baseline:
  - stdout/stderr text shape
  - exit code contract (`invalid-flag.exit-code.txt`)
- Any drift is a hard gate failure.

3. Version output parity
- `forge version` output shape must match oracle baseline.
- Version formatting changes require explicit baseline refresh in same change.

4. JSON and exit-code parity
- For JSON-bearing CLI commands in scope, field-level schema and required keys must match.
- Exit codes must preserve success/non-success behavior for equivalent conditions.
- Gate threshold: 100% parity for in-scope CLI commands (no tolerated drift).

## CI gate wiring

- `parity` workflow job runs:
  - `go test ./internal/parity -run '^TestCLIGateRootOracleBaseline$' -count=1`
- This CLI gate is required before cutover sign-off.
