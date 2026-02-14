# forge-m4p - parity dashboard workflow path fix (2026-02-13)

## Summary

CI parity dashboard step wrote output to `$GITHUB_WORKSPACE/parity-dashboard` but then
read markdown via relative `cat parity-dashboard/parity-dashboard.md` after `cd old/go`.
That caused missing-file failures in CI step summaries.

## Changes

- `.github/workflows/ci.yml`
  - execute parity-dashboard generator in subshell: `(cd old/go && go run ...)`
  - use workspace-absolute summary path:
    - `cat "$GITHUB_WORKSPACE/parity-dashboard/parity-dashboard.md" >> "$GITHUB_STEP_SUMMARY"`
- `.github/workflows/parity-nightly.yml`
  - execute parity-dashboard generator in subshell for consistent cwd behavior.
- Added regression test:
  - `old/go/internal/doccheck/parity_dashboard_workflow_test.go`

## Validation

```bash
cd old/go
gofmt -w internal/doccheck/parity_dashboard_workflow_test.go
env -u GOROOT -u GOTOOLDIR go test ./internal/doccheck -run '^TestParityDashboardWorkflowUsesWorkspaceOutputPaths$' -count=1
```

Result: pass.
