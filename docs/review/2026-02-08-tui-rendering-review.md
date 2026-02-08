# Review: TUI Rendering Upgrade (2026-02-08)

## Scope

- Reviewed no-arg TUI changes in `internal/looptui`.
- Reviewed UX changes (tabs, theming, logs/runs views, multi-pane logs).
- Verified config/docs wiring.

## Findings

### 1. High: tailing logs loaded full file each refresh (fixed)

- Risk: severe refresh latency and memory churn on large logs, multiplied by multi-pane mode.
- Fix: switched `tailFile` to bounded reverse-read with 2MB cap.
- File: `internal/looptui/looptui.go:2012`

### 2. Medium: log timestamp parsing lost indentation context (fixed)

- Risk: inconsistent highlighting on indented timestamped lines.
- Fix: timestamp prefix parser now preserves leading spaces and parses from raw line.
- File: `internal/looptui/highlighter.go:197`

### 3. Medium: discoverability gap for new controls (fixed)

- Risk: many new keys (tabs/layout/source/run selection) were hard to discover.
- Fix: added in-app help overlay (`?`) with contextual shortcuts and escape path.
- Files:
  - `internal/looptui/looptui.go:653`
  - `internal/looptui/looptui.go:1752`

### 4. Medium: tab bar could degrade on narrow terminals (fixed)

- Risk: styled tab bar truncation could become unreadable in narrow widths.
- Fix: added compact fallback labels for constrained widths.
- File: `internal/looptui/looptui.go:1325`

### 5. Minor: run-history scanability could be better (fixed)

- Improvement: run rows now show exit code + duration in addition to status/profile.
- File: `internal/looptui/looptui.go:1531`

## Design Review Notes

- Theme system now coherent and configurable (`default`, `high-contrast`, `ocean`, `sunset`).
- Tabs create clear task modes: inspect loop, inspect logs, inspect runs, compare many loops.
- Multi-log pinning/layout control is useful for orchestration workflows.
- Help overlay and tab hints materially improve first-run usability.

## Residual Risks

- Reverse-tail uses a 2MB cap by design; very large single-line outputs can truncate the earliest visible fragment.
- No pixel-style visual regression checks; terminal rendering still needs manual pass across narrow/wide layouts.

## Validation

- `go test ./...` passed after fixes.
- Added tests:
  - `internal/looptui/looptui_test.go` (help mode, tail behavior, tab/log source flows)
  - `internal/looptui/highlighter_test.go` (event/timestamp parsing)

## Follow-up (User Feedback Pass)

- Added deep log scrolling UX:
  - `pgup/pgdown/home/end` + `u/d` line paging.
  - larger dynamic live-log backfill window for logs tab and expanded viewer.
- Added zen mode (`z`) to expand right pane for log-heavy workflows.
- Increased differentiation between tabs:
  - **Overview** now loop/run snapshot and guidance (not another log view).
  - **Logs** now dedicated live/historical viewport with scroll window metadata.
  - **Runs** now run-first list/details + separately scrollable output area.
