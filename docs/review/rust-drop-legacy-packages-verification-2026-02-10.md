# Drop Legacy-Only Package Ports Verification (2026-02-10)

Task: `forge-q5a`  
Mode: manifest + doccheck evidence

## Evidence

- `docs/rust-port-manifest.md` contains explicit drop sections:
  - `## Drop: legacy command groups`
  - `## Drop: legacy/dead packages`
- `docs/rust-legacy-drop-list.md` is enforced against Go legacy registrations.

## Command run

```bash
rg -n '^## Drop: legacy/dead packages' docs/rust-port-manifest.md \
  && env -u GOROOT -u GOTOOLDIR go test ./internal/doccheck \
    -run '^TestLegacyDropListCoversAddLegacyRegistrations$' -count=1
```

Result: pass

