# Rust Rewrite Parity Matrix (Template)

Task: `forge-ham`
Version: `v0` (2026-02-09)

## Status scale

- `not-started`
- `in-progress`
- `parity-green`
- `blocked`

## Surface matrix

| Surface | Go source of truth | Rust target | Gate artifact | Status | Notes |
|---|---|---|---|---|---|
| Forge CLI help/flags | `cmd/forge`, `internal/cli` | `forge-cli` | parity snapshots + doccheck + `docs/rust-cli-gate.md` | in-progress | CLI gate criteria + CI baseline test wired |
| fmail CLI help/flags | `cmd/fmail`, `internal/fmail` | `fmail-cli` | `docs/forge-mail/help/*` + doccheck | in-progress | command manifest frozen |
| fmail-tui CLI flags | `cmd/fmail-tui`, `internal/fmailtui` | `fmail-tui` | `docs/forge-mail/help/*` + doccheck | in-progress | flag matrix frozen |
| DB migrations/schema | `internal/db/migrations` | `forge-db` | schema fingerprint tests | in-progress | baseline files committed in `internal/parity/testdata/schema/*` |
| Loop runtime semantics | `internal/loop`, `internal/queue` | `forge-loop` | characterization harness + `docs/rust-runtime-gate.md` | in-progress | runtime gate criteria + CI baseline test wired |
| Daemon + runner protocol | `internal/forged`, `internal/agent/runner` | `forge-daemon` + `forge-runner` | proto compat tests + `docs/rust-daemon-proto-gate.md` | in-progress | gate criteria + CI baseline test wired |
| Loop TUI workflows | `internal/looptui` | `forge-tui` | manual + scripted smoke + `docs/rust-loop-tui-gate.md` | in-progress | gate criteria documented; cutover blocked until checklist green |
| fmail/fmail-tui workflows | `internal/fmail`, `internal/fmailtui` | `fmail-core` + `fmail-tui` | scripted + manual checklist | not-started | |

## Required evidence per gate

- CI required checks green (`parity`, `baseline-snapshot`, `rust-quality`, `rust-coverage`).
- Drift artifacts available for failures (`parity-diff`).
- Baseline snapshot artifact published (`rust-baseline-snapshot`).
- Rust coverage policy locked and enforced (`docs/rust-coverage-policy.md`).
- Release gate checklist completed for final switch (`docs/rust-release-gate-checklist.md`).

## Daemon/Proto Gate Criteria (`forge-ynh`)

- Proto contract lock:
  - Source of truth: `proto/forged/v1/forged.proto`.
  - Generated contract files must exist and stay compatible: `gen/forged/v1/forged.pb.go`, `gen/forged/v1/forged_grpc.pb.go`.
- Interop directions required before cutover:
  - Rust client -> Go `forged` server.
  - Go client -> Rust `forge-daemon` server.
- Runner lifecycle parity required:
  - `StartLoopRunner`, `StopLoopRunner`, `GetLoopRunner`, `ListLoopRunners` semantics must match.
- Streaming parity required:
  - `StreamEvents`, `StreamPaneUpdates`, and `StreamTranscript` compatibility and error semantics must match.
- CI baseline gate:
  - `go test ./internal/parity -run '^TestDaemonProtoGate' -count=1` in the `parity` job.

## Change protocol

- Any parity status change requires linked evidence artifact/test.
- New surface rows must include explicit Go source and Rust target owner crate.
