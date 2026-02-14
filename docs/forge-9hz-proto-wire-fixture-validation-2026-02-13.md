# forge-9hz - proto-wire fixture drift validation (2026-02-13)

## Summary

`TestProtoWireGateCriticalRPCFixtures` had reported drift in a prior parity run.
I reran the supported golden-refresh flow and revalidated the proto-wire gate.

## Commands

```bash
cd old/go
FORGE_UPDATE_GOLDENS=1 env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestProtoWireGateCriticalRPCFixtures$' -count=1
env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run 'TestProtoWireGateCriticalRPCFixtures|TestProtoWireGateBaseline' -count=1
```

## Result

- Both proto-wire gate tests pass.
- No additional fixture file delta remained after refresh.
