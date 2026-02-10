# Rust Rewrite Release Notes + Change Communication

Scope: single-switch cutover release (Go -> Rust ownership) and first stable Rust
release after cutover.

Goal: ship a release artifact + story that operators can trust, with clear
upgrade/rollback guidance.

## Owner + sign-off (fill before cutover)

| Role | Name | Time (UTC) | Decision |
|---|---|---|---|
| Release owner | _TBD_ | _TBD_ | _GO/NO-GO_ |
| Ops on-call | _TBD_ | _TBD_ | _GO/NO-GO_ |
| Runtime owner (Rust) | _TBD_ | _TBD_ | _GO/NO-GO_ |
| Runtime owner (Go rollback) | _TBD_ | _TBD_ | _GO/NO-GO_ |

## Required artifacts

- Release gate checklist completed: `docs/rust-release-gate-checklist.md`
- Operator migration guide ready: `docs/rust-operator-migration-guide.md`
- Post-cutover incident/runbook package ready: `docs/rust-post-cutover-incident-runbook.md`
- Parity matrix updated: `docs/rust-parity-matrix.md`

## Release notes checklist (publishable)

Record these before tagging a release:

- Version + date:
  - Rust release: _TBD_
  - Cutover SHA: _TBD_
- Summary (3-6 bullets):
  - _TBD_
- Operator impact:
  - What changes in day-to-day commands? _TBD_
  - Any known behavior changes (intentional drift)? _TBD_
- Upgrade notes:
  - operator migration guidance: `docs/rust-operator-migration-guide.md`
  - config changes? `docs/config.md` deltas: _TBD_
  - database/migrations notes: _TBD_
- Reliability notes:
  - CI gates green + links: _TBD_
  - parity matrix evidence links: _TBD_
- Known issues + mitigations (limit to real known):
  - _TBD_
- Rollback statement:
  - Go rollback supported during stabilization window per ADR:
    `docs/adr/0005-rust-single-switch-policy.md`
  - link runbook: `docs/rust-post-cutover-incident-runbook.md`

## Change communication checklist

Audiences:
- Contributors (local dev)
- Operators (run control plane day-to-day)
- On-call (incident response)

Channels (fill for your org):

| Channel | Audience | Message link | When |
|---|---|---|---|
| GitHub release notes | all | _TBD_ | tag time |
| README/Docs update | contributors/operators | _TBD_ | tag time |
| Internal announcement | operators/on-call | _TBD_ | before cutover |
| On-call handoff note | on-call | _TBD_ | before cutover |

Minimum comms content:
- what changed (Rust is primary)
- what didn't change (CLI surface intended parity for non-legacy)
- where to report regressions (parity playbook + issue/task tracker)
- rollback expectations + how to request rollback

## Templates

### Internal announcement (operators/on-call)

Title: Rust cutover release (single-switch) is scheduled

Body (fill):
- When: _TBD_
- What: Forge runtime ownership switches from Go to Rust (non-legacy parity).
- Gates: `docs/rust-release-gate-checklist.md` (links: _TBD_)
- Smoke checklist: `docs/rust-post-cutover-incident-runbook.md`
- Rollback: supported during stabilization window; triggers + steps in runbook.
- Escalation: _TBD contact_

### GitHub release notes (public-ish)

Title: Forge Rust cutover release

Body (fill):
- Highlights:
  - _TBD_
- Upgrade notes:
  - _TBD_
- Known issues:
  - _TBD_
- Rollback:
  - _TBD (link runbook if appropriate for audience)_
