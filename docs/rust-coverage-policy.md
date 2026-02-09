# Rust Coverage Policy

Tasks: `forge-wmb`, `forge-n99`, `forge-jhp`
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
- Per-crate enforcement: `scripts/rust-coverage-gate.sh` (reads thresholds file, runs `cargo llvm-cov --package <crate> --fail-under-lines <N>`).
- New crates must be added to `rust/coverage-thresholds.txt` in the same PR.

## Workspace enforcement

- CI job: `.github/workflows/ci.yml` -> `rust-coverage`
- Global workspace thresholds enforced via `--fail-under-lines`, `--fail-under-functions`, `--fail-under-regions` in CI.
- CI workflow must install and run `cargo-llvm-cov`.
- CI workflow must produce LCOV at `rust/coverage/lcov.info`.
- CI workflow must upload the `rust-coverage` artifact.

## Coverage gate self-test

- Workflow: `.github/workflows/coverage-gate-self-test.yml`
- Trigger: `workflow_dispatch`
- Behavior: writes intentional failing thresholds (`forge-parity-stub 101`) and asserts `scripts/rust-coverage-gate.sh` exits non-zero.

## Update rules

- Threshold changes require explicit PR rationale.
- Any temporary waiver must be tracked as a separate explicit exception task.

## Drift detection

- `internal/doccheck` tests verify workflow + policy drift.
