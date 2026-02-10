# Rust Post-Cutover Incident + Runbook Package

Scope: first 1-7 days after the **single final switch** from Go to Rust runtime
ownership (ADR: `docs/adr/0005-rust-single-switch-policy.md`).

Goal: detect regressions fast, contain blast radius, and execute rollback
cleanly when needed.

## Roles (fill before cutover)

| Role | Name | Contact | Backup |
|---|---|---|---|
| Release owner | _TBD_ | _TBD_ | _TBD_ |
| Ops on-call | _TBD_ | _TBD_ | _TBD_ |
| Runtime owner (Rust) | _TBD_ | _TBD_ | _TBD_ |
| Runtime owner (Go rollback) | _TBD_ | _TBD_ | _TBD_ |
| Parity reviewer | _TBD_ | _TBD_ | _TBD_ |

## Critical links (fill before cutover)

- Release gate: `docs/rust-release-gate-checklist.md`
- Parity regression playbook: `docs/parity-regression-playbook.md`
- Parity matrix: `docs/rust-parity-matrix.md`
- Runtime gate: `docs/rust-runtime-gate.md`
- Daemon/proto gate: `docs/rust-daemon-proto-gate.md`
- fmail gate: `docs/rust-fmail-gate.md`
- Loop TUI gate/checklist: `docs/rust-loop-tui-gate.md`, `docs/rust-loop-tui-checklist.md`

External:
- CI run (cutover SHA): _TBD_
- Nightly parity run (latest): _TBD_
- Monitoring dashboard(s): _TBD_

## Post-Cutover Smoke Checklist (T+0 to T+30m)

Record evidence:

| Item | Result | Evidence |
|---|---|---|
| Rust binary version is correct | _TBD_ | _TBD_ |
| `forge doctor` clean (or expected warnings) | _TBD_ | _TBD_ |
| DB opens; migrations applied | _TBD_ | _TBD_ |
| Start 1 loop; completes 1 iteration | _TBD_ | _TBD_ |
| Spawn agent (tmux) + stop agent works | _TBD_ | _TBD_ |
| fmail send + log works | _TBD_ | _TBD_ |
| Loop TUI launches; basic navigation | _TBD_ | _TBD_ |

Suggested local commands (adjust for environment):

```bash
forge --version
forge doctor
forge migrate status

# quick functional smoke (examples; adjust to actual CLI)
forge ps
fmail send task "post-cutover smoke: ok"
fmail log task -n 20
```

If any smoke item fails: treat as incident and follow triage below.

## Incident Severity + Decision Rules

SEV0 (rollback likely):
- data corruption, irreversible data loss, or migration break
- daemon/runner cannot spawn/kill agents reliably
- core loop runtime broken (no task progress) across environments
- widespread crash on normal operator workflows

SEV1 (rollback possible; fix-forward if safe):
- parity drift with high user impact but workaround exists
- performance regression that risks timeouts or runaway resource use
- repeated TUI crashes or major UX regressions for operators

SEV2 (fix-forward):
- minor UI issues, small behavior nits, non-critical warnings

Rollback trigger guideline:
- any SEV0 => rollback unless root cause is confidently isolated and
  mitigation verified quickly
- 2+ SEV1 in first 24h => strongly consider rollback

## Incident Triage Checklist

1. Capture environment + version
   - `forge --version`
   - config path used (see `docs/config.md`)
   - OS, tmux version, ssh version
2. Capture logs
   - run failing command with verbose logging if available
   - record stderr/stdout and attach to incident tracker
3. Determine surface owner via parity gates
   - use `docs/parity-regression-playbook.md` mapping
4. Reproduce in smallest scope
   - prefer minimal repro that isolates CLI vs DB vs runtime vs daemon vs fmail
5. Decide fix-forward vs rollback
   - if rollback: execute procedure below; keep incident open until verification

## Common Incidents (First-Response Playbooks)

### Parity regression detected

- Confirm if regression is real vs baseline drift.
- Run parity triage steps: `docs/parity-regression-playbook.md`.
- If regression touches in-scope runtime surfaces (see include matrix:
  `docs/rust-package-include-matrix.md`) and is high impact: consider rollback.

### Migration/db failure

- Check `forge migrate status` + `forge migrate version`.
- If schema cannot be opened or is partially migrated: stop and rollback.
- Preserve DB copy before any repair attempts.

### Loop runtime stuck / no progress

- Check queue semantics + smart-stop behavior: `docs/rust-runtime-gate.md`.
- Verify daemon/runner liveness if agents are involved.
- If no tasks make progress across environments: likely SEV0/SEV1.

### Daemon/runner interop failures

- Verify gRPC/proto compatibility gate: `docs/rust-daemon-proto-gate.md`.
- Test spawn/kill flows; capture tmux session/pane state.
- If spawn/kill unreliable: treat as SEV0.

### fmail send/log broken

- Verify store path and permissions.
- Re-run fmail gate repro checklist: `docs/rust-fmail-gate.md`.

## Rollback Procedure (Rust -> Go)

Policy reference: `docs/adr/0005-rust-single-switch-policy.md`.

Preconditions:
- Go binaries/build path still releasable.
- Rollback owner on-call is present.

Steps (fill exact commands per environment before cutover):

| Step | Command / Action | Evidence |
|---|---|---|
| Freeze deploys and announce rollback intent | _TBD_ | _TBD_ |
| Switch primary artifact/entrypoint to Go | _TBD_ | _TBD_ |
| Verify `forge --version` shows Go build | _TBD_ | _TBD_ |
| Run smoke checklist again | _TBD_ | _TBD_ |
| Document incident timeline + root cause hypothesis | _TBD_ | _TBD_ |

Post-rollback:
- open parity drift tasks for every affected surface
- decide when to attempt roll-forward again (requires new gate evidence)

## Roll-Forward Procedure (Go -> Rust, after rollback)

Requirements:
- all release gates green again (`docs/rust-release-gate-checklist.md`)
- incident action items closed or explicitly accepted
- rehearsal smoke checklist updated with new learnings

