# Rust Baseline Refresh Protocol

Task: `forge-7sd`
Version: `v1` (2026-02-09)

## Goal

Controlled baseline refresh with explicit approval reference and CI artifact trail.

## Commands

Dry-run (detect drift, no baseline update):

```bash
scripts/rust-baseline-refresh.sh --approval forge-7sd --out-dir build/rust-baseline/refresh
```

Apply (generate refresh artifact set):

```bash
scripts/rust-baseline-refresh.sh --approval forge-7sd --apply --out-dir build/rust-baseline/refresh
```

## Approval requirements

- `--approval` is required.
- Accepted formats:
  - `forge-<task-id>`
  - `PR-<number>`
  - `https://github.com/<owner>/<repo>/pull/<number>`
- Invalid approval reference fails immediately.

## CI integration

- `ci.yml` job `baseline-refresh-protocol` runs protocol dry-run with artifact upload:
  - `rust-baseline-refresh-protocol` (`baseline-refresh-report.json`).
- Manual workflow: `.github/workflows/parity-baseline-refresh.yml`
  - `mode=dry-run` for drift checks.
  - `mode=apply` gated behind environment `parity-baseline-refresh`.

## Artifact

`baseline-refresh-report.json` fields:

- `protocol_version`
- `approval_ref`
- `requested_by`
- `mode`
- `allow_drift`
- `drift_detected`
- `snapshot_dir`
- `generated_at`
