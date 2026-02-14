# forge-7pc - baseline snapshot absolute out-dir support (2026-02-13)

## Summary

`scripts/rust-baseline-snapshot.sh` incorrectly prefixed `repo_root` to `out_dir`
unconditionally, so absolute paths failed (`$repo_root/$out_dir/...`).

## Changes

- Updated `scripts/rust-baseline-snapshot.sh`:
  - resolve `out_dir_abs` once
  - support absolute and relative `out_dir`
  - write/check all artifacts via `out_dir_abs`
- Added regression test:
  - `old/go/internal/doccheck/rust_baseline_snapshot_script_test.go`

## Validation

```bash
cd old/go
gofmt -w internal/doccheck/rust_baseline_snapshot_script_test.go
env -u GOROOT -u GOTOOLDIR go test ./internal/doccheck -run '^TestRustBaselineSnapshotScriptSupportsAbsoluteOutDir$' -count=1

cd ..
scripts/rust-baseline-snapshot.sh build/rust-baseline/check --check
scripts/rust-baseline-snapshot.sh "$(mktemp -d)/baseline-abs" --check
```

All commands pass.
