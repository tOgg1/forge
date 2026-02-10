# Rust Operator Migration Guide (Go -> Rust Single Switch)

Scope: operators upgrading to the Rust primary runtime after the final
single-switch cutover.

Policy:
- Single-switch ownership: `docs/adr/0005-rust-single-switch-policy.md`
- Release gates: `docs/rust-release-gate-checklist.md`

## Who this is for

- you run Forge day-to-day (loops, tmux/ssh, daemon/runner, TUI)
- you need a safe upgrade + rollback story

## What changes

- Rust binaries become the primary ownership runtime for in-scope non-legacy
  behavior.
- Legacy-only behavior is explicitly out-of-scope; see drop list:
  `docs/rust-legacy-drop-list.md`.

What should not change (goal):
- parity for non-legacy CLI/runtime/daemon/fmail/tui behavior (see:
  `docs/rust-parity-matrix.md`).

## Prereqs

- `tmux` installed and usable
- `ssh` access to nodes (if using node/daemon workflows)
- config understood: `docs/config.md`
- runbook available: `docs/rust-post-cutover-incident-runbook.md`

## Pre-cutover checklist (operator)

| Item | Result | Evidence |
|---|---|---|
| Record current version (Go) | _TBD_ | _TBD_ |
| Backup config (`~/.config/forge/config.yaml`) | _TBD_ | _TBD_ |
| Backup data dir (`~/.local/share/forge/`) | _TBD_ | _TBD_ |
| Confirm Go rollback binary/artifact available | _TBD_ | _TBD_ |
| Confirm release gate checklist is complete | _TBD_ | _TBD_ |

## Cutover steps (high-level)

Fill exact commands for your environment before running.

| Step | Command / Action | Result | Evidence |
|---|---|---|---|
| Stop/hold running loops (if required) | _TBD_ | _TBD_ | _TBD_ |
| Upgrade/switch to Rust artifact | _TBD_ | _TBD_ | _TBD_ |
| Verify version | `forge --version` | _TBD_ | _TBD_ |
| Run doctor | `forge doctor` | _TBD_ | _TBD_ |
| Run smoke checklist | see runbook | _TBD_ | _TBD_ |

## Post-cutover operator workflow

First 30m:
- execute smoke checklist (`docs/rust-post-cutover-incident-runbook.md`)
- keep rollback owner reachable
- log every anomaly as an incident or follow-up task

First 24h:
- monitor parity drift and failures (see `docs/parity-regression-playbook.md`)
- execute post-release verification checklist:
  `docs/rust-post-release-verification-checklist.md`

## Rollback

Rollback triggers + procedure:
- `docs/rust-post-cutover-incident-runbook.md`

If you rollback:
- preserve logs + DB snapshot
- open tasks per surface owner, with exact file/line/repro evidence

