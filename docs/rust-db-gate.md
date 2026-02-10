# Rust DB Parity Gate

Task: `forge-d08`  
Status: in-progress

## Goal

Define strict, testable criteria for database schema/migration parity before Rust cutover.

## Scope

- Go source contracts:
  - `internal/db/migrations`
  - `internal/db`
- Rust target crate:
  - `forge-db`

## Required gate criteria

1. Migration order and completeness
- Rust migration chain must preserve Go migration ordering semantics.
- No migration skip or reordering is allowed without explicit compatibility decision.

2. Schema fingerprint parity
- Canonical fingerprint baseline files:
  - `internal/parity/testdata/schema/schema-fingerprint.txt`
  - `internal/parity/testdata/schema/schema-fingerprint.sha256`
- CI gate test: `TestSchemaFingerprintBaseline`.
- Any schema drift requires explicit baseline refresh in same change.

3. Drift detection artifacts
- Drift investigations must use parity/baseline artifacts:
  - `parity-diff`
  - `rust-baseline-snapshot`
- DB-related drift cannot be waived silently.

4. Cutover rule
- DB cutover is blocked until schema fingerprint and migration gate criteria are green.

## CI gate wiring

- `parity` workflow job runs:
  - `go test ./internal/parity -run '^TestSchemaFingerprintBaseline$' -count=1`
- `db-compat` workflow job runs:
  - `scripts/rust-db-compat-check.sh`
  - includes:
    - `cargo test -p forge-db --test transaction_parity_test`
    - `cargo test -p forge-cli --test migrate_go_oracle_fixture_test`
    - `env -u GOROOT -u GOTOOLDIR go test ./internal/db -run '^TestGoReadsRustMutatedDB$' -count=1`
- Baseline snapshot job must include schema fingerprint check:
  - `scripts/rust-baseline-snapshot.sh ... --check`
