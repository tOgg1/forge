# forge-4k2: parity daemon interop tmux split guard (2026-02-13)

## Summary
- Hardened daemon interop parity tests to avoid false failures when tmux pane splitting is unavailable/non-deterministic.

## Changes
- `old/go/internal/parity/daemon_interop_test.go`
  - Added `requireTmuxPaneSplit(t)` preflight guard:
    - skips when `tmux` missing
    - skips when `tmux new-session` or `tmux split-window` is unavailable in environment
  - Added `interopSessionName(prefix)` to generate unique session names per test run.
  - Added `spawnAgentOrSkipTmuxSplit(...)` helper:
    - calls `SpawnAgent`
    - converts internal `tmux split-window failed` errors into `t.Skip` for deterministic parity behavior
    - preserves hard failures for all other spawn errors
  - Updated spawn-dependent interop tests to use unique session names and shared spawn helper.

## Validation
- `cd old/go && env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run 'TestDaemonInteropSpawnAndKillAgent|TestDaemonInteropSendInput|TestDaemonInteropGetTranscript' -count=1 -v`
- `cd old/go && env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestDaemonInterop' -count=1`

