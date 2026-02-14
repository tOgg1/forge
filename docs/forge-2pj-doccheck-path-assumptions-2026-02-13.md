# forge-2pj - doccheck path assumptions aligned to repo layout (2026-02-13)

## Summary

`old/go/internal/doccheck` had stale path assumptions after workspace structure changes:

- repo root detection often resolved to `old/go` instead of workspace root
- script checks used `../..` prefixes that escaped the repo
- Rust workspace checks expected `rust/Cargo.toml` and `rust/crates/*`
- lint policy doc still referenced outdated `rust/`-prefixed config paths

## Changes

- Updated doccheck root/module helpers:
  - `old/go/internal/doccheck/fmail_manifest_test.go`
    - `repoRoot` now resolves workspace root (docs + module markers)
    - new `goModuleRoot` helper for Go command execution in `old/go`
    - added `TestRepoRootResolvesWorkspaceRoot`
- Updated command execution helpers to use `goModuleRoot`:
  - `old/go/internal/doccheck/forge_root_snapshot_test.go`
  - `old/go/internal/doccheck/forge_operational_help_snapshots_test.go`
- Updated script path checks to workspace-root paths:
  - `old/go/internal/doccheck/rust_frankentui_pin_check_script_test.go`
  - `old/go/internal/doccheck/rust_baseline_snapshot_script_test.go`
- Updated workspace Cargo/config assumptions:
  - `old/go/internal/doccheck/rust_workspace_lint_policy_test.go`
  - `old/go/internal/doccheck/rust_crate_boundary_policy_test.go`
- Updated lint policy doc paths:
  - `docs/rust-workspace-lint-policy.md`

## Validation

```bash
cd old/go
env -u GOROOT -u GOTOOLDIR go test ./internal/doccheck -count=1
```

Result: pass.
