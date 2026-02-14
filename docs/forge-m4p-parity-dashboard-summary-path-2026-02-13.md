# forge-m4p: parity dashboard summary path fix (2026-02-13)

## Summary
- Fixed parity dashboard workflow steps to avoid cwd side effects from `old/go` module execution.
- Ensured markdown summary cat uses workspace-absolute path in both CI workflows.

## Changes
- `.github/workflows/ci.yml`
  - Run parity dashboard generator in subshell:
    - `(cd old/go && go run ./cmd/parity-dashboard ...)`
  - Append markdown summary via absolute path:
    - `cat "$GITHUB_WORKSPACE/parity-dashboard/parity-dashboard.md" >> "$GITHUB_STEP_SUMMARY"`
- `.github/workflows/parity-nightly.yml`
  - Run parity dashboard generator in subshell.
  - Append markdown summary via the same workspace-absolute path.
- `old/go/internal/doccheck/parity_dashboard_workflow_test.go`
  - Added regression guard for both workflows:
    - requires subshell `cd old/go` execution
    - requires workspace `--out` path
    - rejects relative summary cat path
    - requires absolute summary cat path

## Validation
- `cd old/go && env -u GOROOT -u GOTOOLDIR go test ./internal/doccheck -run TestParityDashboardWorkflowUsesWorkspaceOutputPaths -count=1`
- `cd old/go && env -u GOROOT -u GOTOOLDIR go test ./internal/doccheck -count=1`

