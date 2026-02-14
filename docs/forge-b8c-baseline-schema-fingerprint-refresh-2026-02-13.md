# forge-b8c - baseline schema fingerprint refresh (2026-02-13)

## Summary

`rust-baseline-snapshot` check failed because schema fingerprint fixtures in
`old/go/internal/parity/testdata/schema/` were stale after migrations 14/15
(team model + team tasks).

## Changes

- Regenerated:
  - `old/go/internal/parity/testdata/schema/schema-fingerprint.txt`
  - `old/go/internal/parity/testdata/schema/schema-fingerprint.sha256`

Command used:

```bash
cd old/go
env -u GOROOT -u GOTOOLDIR go run ./cmd/schema-fingerprint --out-dir internal/parity/testdata/schema
```

## Validation

```bash
cd old/go
env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestSchemaFingerprintBaseline$' -count=1

cd ..
scripts/rust-baseline-snapshot.sh build/rust-baseline/check --check
```

Both commands pass.
