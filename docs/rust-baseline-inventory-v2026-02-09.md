# Rust Rewrite Baseline Inventory (v2026-02-09)

Task: `forge-54e`
Baseline tag: `v2026-02-09`

## Scope

Frozen baseline for:
- command surface snapshots,
- DB migration inventory,
- core protocol fingerprint,
- key workflow entrypoint help outputs.

## Artifact pointers

Canonical repo snapshots:
- `docs/forge-mail/help/fmail-help.txt`
- `docs/forge-mail/help/fmail-tui-help.txt`
- `docs/rust-port-manifest.md`
- `docs/rust-fmail-command-manifest.md`
- `docs/rust-legacy-drop-list.md`

Generated baseline bundle (local):
- `build/rust-baseline/latest/forge-help.txt`
- `build/rust-baseline/latest/fmail-help.txt`
- `build/rust-baseline/latest/fmail-tui-help.txt`
- `build/rust-baseline/latest/db-migrations.txt`
- `build/rust-baseline/latest/proto-forged-sha256.txt`
- `build/rust-baseline/latest/go-loc-summary.txt`
- `build/rust-baseline/latest/generated-at.txt`

Generated baseline bundle (CI artifact):
- Workflow: `.github/workflows/ci.yml` job `baseline-snapshot`
- Artifact: `rust-baseline-snapshot`

## Regeneration

```bash
scripts/rust-baseline-snapshot.sh build/rust-baseline/latest --check
```

`--check` performs drift detection against committed fmail help snapshots and
fails on mismatch.

## Drift policy

- Snapshot drift is allowed only with an intentional scope update in the same
  PR.
- Drift PR must update manifest docs and baseline artifacts together.
