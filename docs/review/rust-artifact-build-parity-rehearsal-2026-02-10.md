# Artifact/Build Parity Rehearsal (2026-02-10)

Task: `forge-1s5`  
Mode: baseline snapshot check (Go oracle) + artifact bundle generation

## Command run

```bash
scripts/rust-baseline-snapshot.sh build/rust-baseline/rehearsal-2026-02-10 --check
```

Result: pass

## Artifacts

- Snapshot dir: `build/rust-baseline/rehearsal-2026-02-10/`
- Includes:
  - `forge-help.txt`, `fmail-help.txt`, `fmail-tui-help.txt`
  - `schema-fingerprint.sha256`
  - `db-migrations.txt`, `go-loc-summary.txt`
  - `proto-forged-sha256.txt`
  - `generated-at.txt`

