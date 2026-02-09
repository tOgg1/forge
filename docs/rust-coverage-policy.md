# Rust Coverage Policy

Tasks: `forge-wmb`, `forge-n99`, `forge-tmk`, `forge-jhp`
Date: 2026-02-09

## Tooling decision

- Rust coverage tool: `cargo-llvm-cov`.
- Rationale: LLVM source-based instrumentation gives the most accurate per-line
  coverage; full macOS + Linux support; first-class CI install via
  `taiki-e/install-action`; already integrated in Forge CI.
- Rejected alternatives:
  - `cargo-tarpaulin`: ptrace-based, accuracy issues on macOS and with inlined code.
  - `grcov`: requires manual profdata wrangling, no advantage over cargo-llvm-cov.

## Report format decision

- Machine-readable report format: LCOV.
- Canonical output path (repo-relative): `rust/coverage/lcov.info`.
- CI artifact name: `rust-coverage` (contains `rust/coverage/lcov.info`).
- Human-readable summary: `cargo llvm-cov report --summary-only` appended to `GITHUB_STEP_SUMMARY`.

## Local development

```bash
# Text summary
cd rust && cargo llvm-cov --workspace --all-features

# LCOV file
cd rust && cargo llvm-cov --workspace --all-features --lcov --output-path coverage/lcov.info

# HTML report (opens in browser)
cd rust && cargo llvm-cov --workspace --all-features --html --open
```

## Per-crate thresholds

- Per-crate thresholds are source-controlled in `rust/coverage-thresholds.txt`.
- Per-crate enforcement: `scripts/rust-coverage-gate.sh` (reads thresholds + waivers; runs `cargo llvm-cov --package <crate> --fail-under-lines <N>` when no active waiver exists).
- New crates must be added to `rust/coverage-thresholds.txt` in the same PR.

## Temporary waiver process

- Waiver registry: `rust/coverage-waivers.txt`.
- Waiver row format: `crate|expires_on|approved_by|issue|reason`.
- `expires_on` must be UTC date `YYYY-MM-DD` and cannot be in the past.
- Waivers are temporary only; remove waiver rows as soon as crate thresholds are met.
- `scripts/rust-coverage-gate.sh` validates waiver schema, duplicate rows, expiry, and unknown crate references.
- CI fails if waiver validation fails.
- Each waiver must have an explicit approval owner and tracking issue/task.

## Workspace enforcement

- CI job: `.github/workflows/ci.yml` -> `rust-coverage`
- Nightly coverage publication: `.github/workflows/parity-nightly.yml` -> `rust-coverage-nightly`
- Global workspace thresholds enforced via `--fail-under-lines`, `--fail-under-functions`, `--fail-under-regions` in CI.
- CI workflow must install and run `cargo-llvm-cov`.
- CI workflow must produce LCOV at `rust/coverage/lcov.info`.
- CI workflow must upload the `rust-coverage` artifact.
- Nightly workflow must upload `rust-coverage-nightly` artifact (`rust/coverage/nightly-lcov.info`).

## Coverage gate self-test

- Workflow: `.github/workflows/coverage-gate-self-test.yml`
- Trigger: `workflow_dispatch`
- Behavior: writes intentional failing thresholds (`forge-parity-stub 101`) and asserts `scripts/rust-coverage-gate.sh` exits non-zero.

## Update rules

- Threshold changes require explicit PR rationale.
- Any temporary waiver must be tracked as a separate explicit exception task.

## Drift detection

- `internal/doccheck` tests verify workflow + policy drift.
