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

Automation helper:
- `docs/rust-final-switch-automation.md`

| Step | Command / Action | Result | Evidence |
|---|---|---|---|
| Stop/hold running loops (if required) | _TBD_ | _TBD_ | _TBD_ |
| Upgrade/switch to Rust artifact | `scripts/rust-final-switch.sh cutover --cutover-cmd '<switch-to-rust-command>' --hook 'scripts/rust-final-switch-checklist-hook.sh docs/review/rust-final-switch-checklist-log.md' --log-file docs/review/rust-final-switch-run.log` | _TBD_ | _TBD_ |
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

## Rust-first day-2 workflow

Use these commands as the default operator loop after cutover.

### 1. Spawn / scale

```bash
# initialize repo once
forge init

# start one loop
forge up --name review-loop --profile codex --prompt prompts/review.md --interval 60s

# scale profile/prompt cohort to target count
forge scale --name-prefix review --count 3 --profile codex --prompt prompts/review.md
```

### 2. Health checks

```bash
# environment and dependency diagnostics
forge doctor

# fleet summary
forge status

# per-loop live state
forge ps
forge ps --json

# wait until a loop is ready for next instruction
forge wait --until ready --agent review-loop --timeout 2m
```

### 3. Stop controls

```bash
# graceful stop (end of current iteration)
forge stop review-loop

# immediate stop
forge kill review-loop

# stop cohort by selector
forge stop --profile codex
```

### 4. Recovery playbooks

```bash
# inspect recent run output for failure reason
forge logs review-loop --lines 200

# resume stopped/errored loop
forge resume review-loop

# force-remove broken or stale loop record
forge rm review-loop --force

# remove inactive loop records in bulk (stopped/error)
forge clean --json
```

Recommended incident breadcrumbs per recovery action:
- failing command and exit code
- loop id/name
- key log excerpt (`forge logs --lines 200`)
- whether recovery used `resume`, `kill`, `rm --force`, or `clean`

## Rollback

Rollback triggers + procedure:
- `docs/rust-post-cutover-incident-runbook.md`

If you rollback:
- preserve logs + DB snapshot
- open tasks per surface owner, with exact file/line/repro evidence
