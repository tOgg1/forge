# Rust Cutover Rehearsal Report (Template)

Purpose: standardize evidence + GO/NO-GO for single-switch rehearsal.

## Metadata

- Date (UTC): _TBD_
- Environment: _staging|prod-like|local_
- Candidate SHA: _TBD_
- Rust artifacts: _TBD_
- Go rollback artifacts: _TBD_
- Operator(s): _TBD_

## Preconditions

- Release gate checklist complete: `docs/rust-release-gate-checklist.md`
- Parity matrix updated: `docs/rust-parity-matrix.md`
- Post-cutover incident/runbook ready: `docs/rust-post-cutover-incident-runbook.md`

## Execution log

Record exact commands + timestamps.

1. Pre-cutover snapshot
   - _TBD_
2. Cutover to Rust primary
   - _TBD_
3. Smoke probes (must be green)
   - loops (CLI + TUI)
   - daemon/runner liveness
   - fmail CLI + fmail-tui workflows
4. Stability window (min 30m)
   - _TBD_

## Rollback rehearsal

- Trigger used: _TBD_
- Rollback steps executed: _TBD_
- Total time to restore service: _TBD_
- Any data-loss / protocol drift: _TBD_

## Issues / follow-ups

| Severity | Area | Symptom | Link / task | Owner |
|---|---|---|---|---|
| _TBD_ | _TBD_ | _TBD_ | _TBD_ | _TBD_ |

## GO/NO-GO summary

| Role | Name | Time (UTC) | Decision | Notes |
|---|---|---|---|---|
| Release owner | _TBD_ | _TBD_ | _GO/NO-GO_ | _TBD_ |
| Runtime owner (Rust) | _TBD_ | _TBD_ | _GO/NO-GO_ | _TBD_ |
| Runtime owner (Go rollback) | _TBD_ | _TBD_ | _GO/NO-GO_ | _TBD_ |
| Parity reviewer | _TBD_ | _TBD_ | _GO/NO-GO_ | _TBD_ |
| Ops on-call | _TBD_ | _TBD_ | _GO/NO-GO_ | _TBD_ |

## Evidence bundle

Attach paths/URLs (CI runs, logs, snapshots).

- _TBD_

