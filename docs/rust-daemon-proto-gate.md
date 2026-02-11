# Rust Daemon/Proto Parity Gate

Task: `forge-ynh`  
Status: in-progress

## Goal

Define strict, testable gate criteria for daemon/proto parity before Rust cutover.

## Scope

- Go source contracts:
  - `proto/forged/v1/forged.proto`
  - `internal/forged`
  - `internal/agent/runner`
- Rust target crates:
  - `forge-daemon`
  - `forge-runner`

## Required gate criteria

1. Proto contract lock
- Proto source file is the authority: `proto/forged/v1/forged.proto`.
- Generated Go compatibility files must exist and stay aligned:
  - `gen/forged/v1/forged.pb.go`
  - `gen/forged/v1/forged_grpc.pb.go`
- Baseline CI test: `TestDaemonProtoGateProtoSurfaceLocked`.

2. Interop direction parity
- Rust client -> Go server must pass for critical unary RPCs:
  - `SpawnAgent`, `KillAgent`, `SendInput`, `ListAgents`, `GetAgent`
  - `StartLoopRunner`, `StopLoopRunner`, `GetLoopRunner`, `ListLoopRunners`
  - `Ping`, `GetStatus`
- Go client -> Rust server must pass for the same RPC set.
- Failure in either direction blocks cutover.

3. Runner lifecycle parity
- Loop runner ownership/liveness semantics must match Go behavior.
- Required parity for runner state transitions and error envelopes.
- Lifecycle regression suites:
  - `rust/crates/forge-daemon/src/loop_runner/tests.rs`
  - `rust/crates/forge-runner/src/runner.rs`
  - `rust/crates/forge-loop/src/stale_runner.rs`

4. Streaming parity
- `StreamEvents`, `StreamPaneUpdates`, and `StreamTranscript` must preserve:
  - wire compatibility,
  - event ordering guarantees,
  - terminal/error semantics.

5. Proto wire baseline fixtures
- Critical unary RPC response wire baselines are source-controlled:
  - `internal/parity/testdata/oracle/expected/forged/proto-wire/summary.json`
  - `internal/parity/testdata/oracle/actual/forged/proto-wire/summary.json`
- Baseline tests:
  - `TestProtoWireGateCriticalRPCFixtures`
  - `TestProtoWireGateBaseline`
- Covered RPC fixtures include:
  - `SpawnAgent`, `KillAgent`, `SendInput`, `StartLoopRunner`, `StopLoopRunner`, `GetStatus`, `Ping`.

## CI gate wiring

- `parity` workflow job runs:
  - `go test ./internal/parity -run '^TestDaemonProtoGate' -count=1`
  - `go test ./internal/parity -run '^TestProtoWireGate' -count=1`
  - `scripts/rust-daemon-runtime-parity.sh`
- Local Rust validation command:
  - `cargo test -p forge-daemon loop_runner`
  - `cargo test -p forge-daemon loop_runner_lifecycle_via_rpc_handlers`
  - `cargo test -p forge-daemon --test rforged_binary_test`
  - `cargo test -p forge-runner runner`
  - `cargo test -p forge-loop stale_runner`
- This baseline gate is required in PR CI.

## Evidence for final cutover

- CI logs showing daemon/proto gate tests green.
- Artifact bundle documenting both interop directions:
  - Rust client -> Go server
  - Go client -> Rust server
- Runner lifecycle scenario parity evidence attached to parity matrix updates.
