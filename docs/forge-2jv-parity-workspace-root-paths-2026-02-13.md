# forge-2jv - parity tests workspace-root path assumptions (2026-02-13)

## Summary

Updated `old/go/internal/parity` tests to resolve paths from workspace root (repo top-level) instead of assuming `old/go` as repo root.

## Changes

- `old/go/internal/parity/daemon_proto_gate_test.go`
  - added `workspaceRoot(t)` helper (derives top-level root from `repoRoot(t)` and validates `docs/` exists)
- `old/go/internal/parity/fmail_gate_test.go`
  - switched baseline docs path root from `repoRoot(t)` to `workspaceRoot(t)`
- `old/go/internal/parity/baseline_refresh_script_test.go`
  - switched `rust-baseline-refresh.sh` lookup to `workspaceRoot(t)/scripts/...`
  - removed obsolete `runtime.Caller`-based root helper
- `old/go/internal/parity/surface_gate_test.go`
  - switched Rust surface build directory to workspace root
  - switched Rust alias extraction path to `crates/forge-cli/src/lib.rs`

## Validation

```bash
cd old/go
env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run 'TestFmailGateCommandAndTUIBaseline|TestBaselineRefreshScriptDryRunPass|TestBaselineRefreshScriptDryRunFailAndAllowDrift|TestBaselineRefreshScriptRejectsInvalidApproval|TestSurfaceGate|TestDaemonProtoGateProtoSurfaceLocked' -count=1
```

Result: pass.
