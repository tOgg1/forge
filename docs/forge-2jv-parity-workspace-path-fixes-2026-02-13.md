# forge-2jv - parity workspace-path fixes (2026-02-13)

## Summary

Several `old/go/internal/parity` tests assumed `old/go` was the workspace root.
That broke script/doc path checks and Rust surface build paths after repo layout
consolidation.

## Changes

- Added `workspaceRoot(t)` helper in `old/go/internal/parity/daemon_proto_gate_test.go`
  to resolve repo root from Go-module root.
- Updated parity tests to use workspace-root paths where required:
  - `old/go/internal/parity/fmail_gate_test.go` (docs under `docs/`)
  - `old/go/internal/parity/baseline_refresh_script_test.go` (scripts under `scripts/`)
  - `old/go/internal/parity/surface_gate_test.go`
    - Rust build dir now workspace root
    - Rust CLI source path now `crates/forge-cli/src/lib.rs`

## Validation

```bash
cd old/go
env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run 'TestBaselineRefreshScriptDryRunPass|TestBaselineRefreshScriptDryRunFailAndAllowDrift|TestBaselineRefreshScriptRejectsInvalidApproval|TestFmailGateCommandAndTUIBaseline' -count=1

cd ..
test -f crates/forge-cli/src/lib.rs
cargo build -p forge-cli --quiet
```

Notes:
- Targeted parity tests pass.
- Full `TestSurfaceGateGoVsRust` is heavy in this harness; path prerequisites now resolve.
