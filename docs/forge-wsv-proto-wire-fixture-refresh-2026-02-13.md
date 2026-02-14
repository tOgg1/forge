# forge-wsv - parity proto-wire critical RPC fixtures (2026-02-13)

## Summary

Investigated `old/go/internal/parity` failure at `TestProtoWireGateCriticalRPCFixtures` (`proto wire fixture drift`).

## Validation

```bash
cd old/go
env -u GOROOT -u GOTOOLDIR FORGE_UPDATE_GOLDENS=1 go test ./internal/parity -run '^TestProtoWireGateCriticalRPCFixtures$' -count=1
env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestProtoWireGateCriticalRPCFixtures$' -count=3
env -u GOROOT -u GOTOOLDIR go test ./internal/parity -count=1
```

All commands passed.

## Notes

- `FORGE_UPDATE_GOLDENS=1` run completed successfully.
- No further fixture/content diff remained after refresh; full `internal/parity` suite is green.
