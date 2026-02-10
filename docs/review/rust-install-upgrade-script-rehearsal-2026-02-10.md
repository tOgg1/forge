# Install/Upgrade Script Rehearsal (2026-02-10)

Task: `forge-074`  
Mode: local static validation + checksum verification

## Fix

- Updated `scripts/bootstrap.sh.sha256` to match current `scripts/bootstrap.sh`.
- Without this, `scripts/install.sh` would fail checksum verification at runtime.

## Command run

```bash
bash -n scripts/install-linux.sh \
  && bash -n scripts/install.sh \
  && (cd scripts && shasum -a 256 -c bootstrap.sh.sha256) \
  && rg -n 'install-linux\\.sh' README.md
```

Result: pass

