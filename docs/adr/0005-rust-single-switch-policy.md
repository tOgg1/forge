# ADR 0005: Rust Rewrite Single-Switch Policy

## Status

Accepted

## Context

Forge is rewriting core behavior to Rust with strict non-legacy parity. A
staggered production cutover increases dual-runtime drift risk and unclear
ownership. We need one explicit cutover point with objective gate evidence and
a tested rollback path.

## Decision

Use a **single final switch** from Go runtime ownership to Rust runtime
ownership.

Prerelease parity gates (must all pass before cutover):
- PR CI: `parity`, `rust-quality`, `rust-coverage` required and green.
- Nightly parity workflow green with no unresolved drift deltas.
- Parity matrix evidence recorded for CLI, DB, loop runtime, daemon/runner, and
  fmail/fmail-tui critical flows.
- Release gate checklist completed with mandatory sign-offs:
  `docs/rust-release-gate-checklist.md`.
- Legacy drop list and command manifests frozen (`docs/rust-legacy-drop-list.md`,
  `docs/rust-port-manifest.md`, `docs/rust-fmail-command-manifest.md`).

Rollback policy:
- Keep Go binaries/build path releasable until post-cutover stabilization
  criteria are met.
- Define and test one-command rollback procedure that restores Go ownership.
- Preserve parity artifacts and logs per release candidate for cutover audit.

## Consequences

- Pros: clear ownership boundary, lower mixed-runtime ambiguity, auditable
  go/no-go decision.
- Cons: larger pre-cutover integration batch; requires strict gate discipline.

## Alternatives considered

- Incremental command-by-command production switch: reduced batch size, but high
  drift and ownership ambiguity during migration.
- Big-bang switch without gate evidence: faster path, but unacceptable rollback
  and parity risk.

## Review checklist

- [x] Cutover trigger is explicit and objective.
- [x] Required parity evidence defined.
- [x] Rollback plan requires pre-validation.
- [x] Scope docs referenced and aligned.
