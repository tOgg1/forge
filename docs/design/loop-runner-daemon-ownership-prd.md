# PRD: Daemon-Owned Loop Runners

Status: Draft  
Owner: Forge core  
Date: 2026-02-08

## Problem

`forge up` starts `forge loop run <id>` as child process of the invoking CLI process. In nested harness/agent contexts, parent cleanup can kill this child process tree. Result: loop record says `running`, but runner is dead.

Current code paths:
- `internal/cli/loop_up.go` (`startLoopProcess`)
- `internal/cli/loop_scale.go` (new loop spawn)
- `internal/cli/loop_resume.go` (resume spawn)
- `internal/looptui/looptui.go` (`startLoopProcess`)

## Goal

Make loop ownership independent from ephemeral caller processes.

Primary goal:
- loops started via CLI remain alive after caller exits.

Strategic goal:
- support `forged` as authoritative owner of loop runner lifecycle.

## Non-Goals

- rewrite loop runtime logic (`internal/loop/runner.go`)
- change prompt semantics / stop-rule semantics
- migrate old agent/tmux orchestration

## User Stories

- As an operator, when I run `forge up` from inside an AI agent shell, loops keep running after that shell exits.
- As an operator, I can choose spawn owner explicitly (`local` vs `daemon`).
- As an operator, `forge ps` reflects true live state, not stale `running`.

## Requirements

### R1 Immediate Reliability (MVP patch)

- `startLoopProcess` must detach runner from parent process group/session.
- Must apply to all spawn entry points (`up`, `scale`, `resume`, TUI spawn helper).
- No behavior change to loop config, queue, memory, or stop rules.

### R2 Daemon-Owned Mode

Add daemon API for loop runners:
- `StartLoopRunner(loop_id, config_path)`
- `StopLoopRunner(loop_id, force)`
- `GetLoopRunner(loop_id)`
- `ListLoopRunners()`

Then add CLI mode selection:
- `forge up --spawn-owner local|daemon|auto`
- same for `scale` and `resume`
- default: `auto`

`auto` policy:
- if local forged reachable: use daemon
- else fallback local detached spawn

### R3 State Integrity

- Add liveness reconciliation step for `forge ps`/status path:
- if loop state `running` but PID missing/dead and no daemon runner, mark `stopped` with reason `stale_runner`.
- reconciliation must be idempotent.

### R4 Observability

- record spawn owner in loop metadata:
- `runner_owner: local|daemon`
- `runner_instance_id` (daemon mode)
- surface in JSON output for diagnostics.

## UX / CLI

Examples:
- `forge up --profile codex1 --spawn-owner auto`
- `forge up --spawn-owner daemon`
- `forge ps --json` includes `runner_owner`, liveness flags.

Error handling:
- `--spawn-owner daemon` and daemon unavailable => hard error.
- `--spawn-owner auto` and daemon unavailable => warn + local detached fallback.

## Success Metrics

- 0 reproducible cases of loops dying when parent shell exits (smoke test).
- `forge ps` stale-running mismatch rate < 1% over 7 days of loop launches.
- Nested-harness `forge up` success parity with direct terminal launches.

## Test Plan

Unit:
- spawn helper sets detached process attrs.
- owner-selection policy (`auto/daemon/local`).
- stale runner reconciliation logic.

Integration:
- start loop from subprocess, kill caller, verify loop continues.
- daemon available path: loop owned by daemon.
- daemon unavailable path: `auto` fallback works, `daemon` fails.

Manual smoke:
- run from codex/claude loop harness, watch `forge ps` + logs for 10+ minutes.

## Rollout

Phase 1:
- ship local detached spawn patch + tests.

Phase 2:
- ship daemon loop RPC + CLI owner mode + fallback policy.

Phase 3:
- enable `auto` default everywhere; add stale-state reconciliation on status paths.

## Risks

- cross-platform detach differences (macOS/Linux)
- duplicate runners for same loop ID if spawn races not guarded
- stale PID checks can false-negative if PID reused (mitigate with start timestamp/metadata)

## Open Questions

- Should daemon loop ownership be per-node only, or also remote-control from main CLI process for non-local repos?
- Where to enforce single-runner lock: DB row lock vs daemon-side mutex vs both?
- Should reconciliation run only on read (`ps`) or also periodic background sweep?
