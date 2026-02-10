# Rust Final Switch Release Gate Checklist

Task: `forge-y1j`  
Status: draft for enforcement

## Gate policy

- Single-switch release is blocked unless every required gate below is green and
  every sign-off field is completed.
- Any missing artifact or missing sign-off is an automatic `NO-GO`.

## Required evidence bundle

| Gate | Required evidence | Source |
|---|---|---|
| PR parity gate | `parity` job green | `.github/workflows/ci.yml` |
| Rust quality gate | `rust-quality` job green | `.github/workflows/ci.yml` |
| Rust coverage gate | `rust-coverage` job green + coverage artifacts | `.github/workflows/ci.yml` |
| Nightly parity stability | latest nightly parity run green | `.github/workflows/parity-nightly.yml` |
| Baseline snapshot | `rust-baseline-snapshot` artifact present | `.github/workflows/ci.yml` |
| CLI/DB/runtime/daemon/fmail parity matrix | updated parity rows + linked evidence | `docs/rust-parity-matrix.md` |
| Daemon/proto gate | daemon/proto gate test + interop criteria met | `docs/rust-daemon-proto-gate.md` |
| Legacy scope freeze | drop list + command manifests frozen | `docs/rust-legacy-drop-list.md`, `docs/rust-port-manifest.md`, `docs/rust-fmail-command-manifest.md` |
| Operator migration guide | operator-facing cutover/rollback guidance published | `docs/rust-operator-migration-guide.md` |
| Post-cutover incident/runbook package | runbook + rollback checklist ready for on-call use | `docs/rust-post-cutover-incident-runbook.md` |

## Mandatory sign-offs

| Role | Required | Name | Time (UTC) | Decision |
|---|---|---|---|---|
| Release owner | yes | _TBD_ | _TBD_ | _GO/NO-GO_ |
| Runtime owner (Rust) | yes | _TBD_ | _TBD_ | _GO/NO-GO_ |
| Runtime owner (Go rollback) | yes | _TBD_ | _TBD_ | _GO/NO-GO_ |
| Parity reviewer | yes | _TBD_ | _TBD_ | _GO/NO-GO_ |
| Operations on-call | yes | _TBD_ | _TBD_ | _GO/NO-GO_ |

## Rehearsal checklist (staging)

1. Freeze candidate SHA and artifact set.
2. Verify required evidence bundle is complete.
3. Run cutover rehearsal with Rust binaries as primary.
4. Execute rollback rehearsal back to Go binaries.
5. Confirm no data-loss and no protocol drift.
6. Record outcomes and attach logs.

## Cutover decision rule

- `GO` only if every mandatory sign-off is `GO`.
- If any sign-off is `NO-GO`, postpone release and open blocker tasks with
  exact failed gate references.
