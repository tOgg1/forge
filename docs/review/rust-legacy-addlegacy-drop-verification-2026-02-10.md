# Legacy `addLegacyCommand(...)` Drop Verification (2026-02-10)

Task: `forge-3z1`  
Mode: doccheck-backed evidence

## Command run

```bash
env -u GOROOT -u GOTOOLDIR go test ./internal/doccheck \
  -run '^TestLegacyDropListCoversAddLegacyRegistrations$' -count=1
```

Result: pass

## Notes

- Legacy command groups are defined by `internal/cli/*` registrations via `addLegacyCommand(...)`.
- Source-of-truth for dropped legacy groups: `docs/rust-legacy-drop-list.md` (enforced by doccheck test).

