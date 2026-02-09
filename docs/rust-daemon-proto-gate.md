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

4. Streaming parity
- `StreamEvents`, `StreamPaneUpdates`, and `StreamTranscript` must preserve:
  - wire compatibility,
  - event ordering guarantees,
  - terminal/error semantics.

## CI gate wiring

- `parity` workflow job runs:
  - `go test ./internal/parity -run '^TestDaemonProtoGate' -count=1`
- This baseline gate is required in PR CI.

## Evidence for final cutover

- CI logs showing daemon/proto gate tests green.
- Artifact bundle documenting both interop directions:
  - Rust client -> Go server
  - Go client -> Rust server
- Runner lifecycle scenario parity evidence attached to parity matrix updates.
