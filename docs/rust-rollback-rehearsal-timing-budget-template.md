# Rust -> Go Rollback Rehearsal Timing Budget (Template)

Goal: make rollback repeatable; measure time-to-safe-state; identify slow steps.

Prereqs:
- `docs/rust-post-cutover-incident-runbook.md`
- Go rollback artifacts ready + accessible
- Rust cutover SHA recorded

## Target budgets (fill before rehearsal)

| Phase | Target | Notes |
|---|---:|---|
| Detect incident + declare SEV | _TBD_ | |
| GO/NO-GO decision to rollback | _TBD_ | |
| Switch artifact/entrypoint | _TBD_ | |
| Smoke verification (post-rollback) | _TBD_ | |
| Incident write-up + follow-up tasks | _TBD_ | |

## Rehearsal log (record times UTC)

| Timestamp (UTC) | Event | Evidence |
|---|---|---|
| _TBD_ | Rehearsal start | _TBD_ |
| _TBD_ | Rollback decision | _TBD_ |
| _TBD_ | Go artifact active | _TBD_ |
| _TBD_ | Smoke green | _TBD_ |
| _TBD_ | Rehearsal end | _TBD_ |

## Findings

- slow step(s): _TBD_
- missing automation: _TBD_
- docs gaps: _TBD_

