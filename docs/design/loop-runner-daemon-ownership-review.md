# Loop Runner Daemon Ownership: Review Notes

Date: 2026-02-08
Reviewer: codex

## PRD review summary

- `R1` clear and actionable: detach local spawn from caller session/process group.
- `R2` clear API surface: loop-runner daemon RPC + CLI owner selection (`local|daemon|auto`).
- `R3` mostly clear; one ambiguity resolved in implementation:
  - when daemon is unreachable, reconciliation avoids force-marking daemon-owned loops as stale to reduce false positives.
- `R4` clear: runner ownership metadata and liveness diagnostics surfaced in JSON.

## Implemented scope

- Detached local spawn for CLI and loop TUI helpers.
- New forged RPCs:
  - `StartLoopRunner`
  - `StopLoopRunner`
  - `GetLoopRunner`
  - `ListLoopRunners`
- CLI flags:
  - `forge up --spawn-owner local|daemon|auto`
  - `forge loop scale --spawn-owner local|daemon|auto`
  - `forge loop resume --spawn-owner local|daemon|auto`
- default mode: `local`
- `auto` behavior:
  - try daemon first
  - on daemon failure: warn + local detached fallback
- Reconciliation in `forge ps`:
  - running + dead/missing PID + no daemon runner => mark `stopped`, reason `stale_runner`
- Metadata/observability:
  - `runner_owner`
  - `runner_instance_id`
  - `forge ps --json` includes owner/instance plus liveness flags.

## Validation

- `go test ./...` pass
- `go build ./cmd/forge ./cmd/forged` pass
