# Rust Runtime Parity Gate

Task: `forge-7j4`  
Status: in-progress

## Goal

Define strict, testable runtime parity criteria for queue handling, smart-stop,
logging, and ledger behavior before Rust cutover.

## Scope

- Go source workflows:
  - `internal/loop`
  - `internal/queue`
  - `internal/scheduler`
- Rust target crate:
  - `forge-loop`

## Required gate criteria

1. Queue semantics parity
- Pending queue items are consumed in stable order.
- `pause`, `stop`, and `kill` control items preserve Go behavior.
- Override prompt + message append behavior remains equivalent.

2. Smart-stop parity
- Quant/condition stop rules preserve decision behavior (`stop` vs `continue`).
- Exit-code/stream/regex matching behavior must stay equivalent.
- Re-queue semantics for unmet conditions must remain stable.

3. Logging and ledger parity
- Run output tail inclusion and line capping remain stable.
- Ledger entry metadata fields remain present and ordered for audit use.
- Optional git summary behavior (`status`/`diff --stat`) remains stable.

4. Runtime dispatch parity
- Idle-only dispatch, cooldown blocking, and conditional re-queue behavior must
  remain equivalent.
- Runtime gate is a hard blocker for final switch.

## CI gate wiring

- `parity` workflow job runs:
  - `go test ./internal/parity -run '^TestRuntimeGateLoopQueueSmartStopLedger$' -count=1`
- This runtime baseline gate is required in PR CI.

## Characterization Fixtures

- Rust fixture suite: `rust/crates/forge-loop/tests/runtime_characterization_fixture_test.rs`
- Fixture source file: `rust/crates/forge-loop/testdata/runtime_characterization_fixture.json`
- Go fixture inputs consumed by the suite:
  - `internal/testutil/testdata/transcripts/claude_code_idle.txt`
  - `internal/testutil/testdata/transcripts/awaiting_approval.txt`
- Validation command:
  - `cargo test -p forge-loop runtime_characterization`

## Evidence for final cutover

- Green CI run for runtime gate baseline test.
- Parity matrix runtime row references this gate doc.
- Release checklist runtime section links runtime gate evidence.
