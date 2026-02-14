# FrankenTUI Rewrite Feature Brainstorm (2026-02-13)

Source thread: `tui-visual-polish` (full correspondence review + follow-up brainstorm round).

## Correspondence synthesis

1. Decision is locked: full rewrite on true FrankenTUI path (legacy path treated as throwaway during migration).
2. Guardrails are stable across agents:
- WI-0 safety first (Unicode-safe truncation + narrow-width behavior)
- parity gates before default flip (`80x24`, `120x40`, `200x50`)
- no implicit interactive fallback (explicit dev-only fallback allowed)
- per-work-item commits
3. Workstream owner/ETA map already posted and partially ACKed in-thread.

## Feature shortlist for world-class + easy-use UX

### P0 (align with WS execution)

1. Runs triage table + sticky output
- Value: fastest path from failing run discovery to actionable output.
- FrankenTUI surface: `Table` + `Panel` + `Flex(vertical)`.
- Metric: failing run output reachable in `<=10s` keyboard-only (p95).
- Risk: narrow-width truncation ambiguity.

2. Incident-first inbox queue
- Value: first-scan prioritization of unread/urgent threads.
- FrankenTUI surface: `List` + `Badge` + `Panel` + `Flex(horizontal)`.
- Metric: highest-priority unread thread found in `<=3` keypresses.
- Risk: metadata discoverability loss (mitigate with compact ID suffix).

3. Multi-loop health wall
- Value: detect degraded loop quickly across fleet.
- FrankenTUI surface: `Flex`/grid split + `Panel` + `LogViewer` + `Badge`.
- Metric: degraded loop among 6 detected in `<=5s` scan.
- Risk: visual noise/color overload.

4. Command surface bar
- Value: cold-start discoverability without reading docs.
- FrankenTUI surface: `StatusLine` + `StatusItem::KeyHint` + `Spacer`.
- Metric: first-time user completes tab switch + filter + palette in `<60s`.
- Risk: hint overload.

5. Modal action rail (safe-by-default actions)
- Value: reduce accidental destructive ops.
- FrankenTUI surface: `ModalStack` + `NotificationQueue` + `Panel`.
- Metric: zero accidental destructive confirms in smoke runs.
- Risk: modal churn.

6. Failure Explain Strip (candidate from ACK follow-up)
- Value: immediate triage hints for selected failure.
- FrankenTUI surface: status/footer strip (`StatusLine`) with deterministic top-3 causes.
- Metric: operator identifies likely root-cause class without context switching.
- Risk: noisy/incorrect heuristics if rules too broad.

### P1 (post-cutover fast-follow)

1. Markdown-rendered inbox detail (`ftui-extras/markdown`).
2. Syntax-highlighted logs (`ftui-extras/syntax` + existing semantic spans).
3. Clipboard copy flow for IDs/log lines (`ftui-extras/clipboard`).
4. Run-output diff view (dual-run compare, bounded memory).
5. Export snapshots (HTML/SVG/text).
6. Forms-based wizard + inline validation.

### P2 (later differentiators)

1. Focus graph traversal across panels.
2. Virtualized long lists/logs everywhere.
3. Per-panel error boundaries.
4. Inspector/layout debug overlay.
5. PTY live attach mode.
6. Agent activity timeline/Gantt style view.

## Integration notes from brainstorm

1. Keep runtime `Model` ownership in WS-1; WS-2 should stay widget/style bridge + parity tests.
2. Prefer enabling upstream default after WS-1 runtime smoke pass (or keep explicit feature until then).
3. Add event-parity tests in bridge gates (keyboard/resize/focus), not only cell-style parity.
4. Preserve architecture seam for future run-diff and PTY source swapping.

## Consensus status (as of 2026-02-13 19:14 local)

- Shortlist posted in-thread.
- Explicit ACK received from `spry-todd` with one addition (`Failure Explain Strip`).
- Awaiting more ACK/BLOCKED responses from remaining WS owners.
