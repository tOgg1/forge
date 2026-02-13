# TUI-907 visual parity checklist and target screenshots

Task: `forge-qst`  
Status: delivered

## Purpose

Blocking visual gate for `tui-superdash` parity completion.

- Compare FrankenTUI shell against legacy `old/go/internal/looptui/looptui.go`.
- Enforce pane/state/interaction coverage.
- Define deterministic screenshot targets for 3 terminal sizes: `80x24`, `120x40`, `200x50`.

## Legacy baseline references

Use these as source-of-truth behavior anchors during parity review:

- Header/tab/status shell: `old/go/internal/looptui/looptui.go:1613`, `old/go/internal/looptui/looptui.go:1656`, `old/go/internal/looptui/looptui.go:2331`
- Pane routing + split structure: `old/go/internal/looptui/looptui.go:1815`
- Overview pane: `old/go/internal/looptui/looptui.go:1852`
- Logs pane: `old/go/internal/looptui/looptui.go:1907`
- Runs pane: `old/go/internal/looptui/looptui.go:1920`
- Multi logs pane: `old/go/internal/looptui/looptui.go:1970`
- Help and wizard overlays: `old/go/internal/looptui/looptui.go:2286`, `old/go/internal/looptui/looptui.go:2214`

## Screenshot capture contract

All targets below must produce **paired evidence**:

- `legacy`: capture from Go looptui baseline.
- `superdash`: capture from Rust FrankenTUI (`forge-tui`).

Evidence naming:

- `build/parity/superdash/<target-id>.legacy.txt`
- `build/parity/superdash/<target-id>.superdash.txt`

Sizing rules:

- Exact terminal sizes: `80x24`, `120x40`, `200x50`.
- No wrapping drift allowed in header/tab/status lines.
- If line trimming occurs, both legacy and superdash must trim at same semantic boundary.

## Global visual hierarchy gate

Pass required for every screenshot target:

- [ ] Header line present and informative (`tab`, `counts`, `theme`, `mode/focus`).
- [ ] Navigation affordance visible (tab rail or deep-focus equivalent).
- [ ] Primary content region visually dominant over hints/footer text.
- [ ] Status strip visible and readable.
- [ ] Focus target visible (selected row, active tab, or selected overlay row).
- [ ] ANSI16/256/truecolor all preserve hierarchy contrast (accent > primary > muted).

## Pane parity targets

### Overview

- [ ] `QST-OVERVIEW-80x24-BASE`
- [ ] `QST-OVERVIEW-120x40-BASE`
- [ ] `QST-OVERVIEW-200x50-BASE`
- [ ] `QST-OVERVIEW-80x24-EMPTY` (no loops; creation guidance visible)
- [ ] `QST-OVERVIEW-80x24-ERROR` (error banner/line visible, non-crashing)

Required visible signals:

- Loop identity/status summary.
- Run snapshot block.
- Workflow jump hint (`Logs/Runs/Multi Logs`).

### Logs

- [ ] `QST-LOGS-80x24-BASE`
- [ ] `QST-LOGS-120x40-BASE`
- [ ] `QST-LOGS-200x50-BASE`
- [ ] `QST-LOGS-80x24-LAYER-ERRORS`
- [ ] `QST-LOGS-80x24-SOURCE-CYCLE` (live/latest-run/selected-run label changes)

Required visible signals:

- Source label + layer label.
- Scroll window indicator/top-bottom behavior.
- Readable error/tool/diff emphasis.

### Runs

- [ ] `QST-RUNS-80x24-BASE`
- [ ] `QST-RUNS-120x40-BASE`
- [ ] `QST-RUNS-200x50-BASE`
- [ ] `QST-RUNS-80x24-EMPTY`

Required visible signals:

- Run timeline rows with selected run marker.
- Exit-state badges/chips.
- Linked run detail/log context.

### Multi Logs

- [ ] `QST-MULTI-80x24-BASE`
- [ ] `QST-MULTI-120x40-BASE`
- [ ] `QST-MULTI-200x50-BASE`
- [ ] `QST-MULTI-80x24-PINNED-FIRST`
- [ ] `QST-MULTI-80x24-EMPTY-CELL`

Required visible signals:

- Layout label + page index.
- Sticky per-loop mini headers.
- Pinned ordering and empty-cell guidance.

### Inbox

- [ ] `QST-INBOX-80x24-BASE`
- [ ] `QST-INBOX-120x40-BASE`
- [ ] `QST-INBOX-200x50-BASE`
- [ ] `QST-INBOX-80x24-CONFLICT`
- [ ] `QST-INBOX-80x24-HANDOFF-SNAPSHOT`

Required visible signals:

- Thread list + detail pane.
- Claim timeline/conflict marker readability.
- Handoff package preview with task/loop links.

## Interaction and overlay targets

- [ ] `QST-HELP-80x24`
- [ ] `QST-COMMAND-PALETTE-120x40`
- [ ] `QST-SEARCH-120x40`
- [ ] `QST-FILTER-80x24`
- [ ] `QST-CONFIRM-80x24`
- [ ] `QST-WIZARD-120x40-STEP1`
- [ ] `QST-WIZARD-120x40-STEP4-REVIEW`
- [ ] `QST-FOCUS-DEEP-120x40`
- [ ] `QST-DENSITY-COMPACT-120x40`
- [ ] `QST-ONBOARDING-OVERLAY-120x40`

Required visible signals:

- Keyboard-first affordances + focus indicator.
- Overlay layering does not hide critical status context.
- Help text and footer hints stay legible at `80x24`.

## State coverage gate (must pass before task closure in epic)

- [ ] Empty state coverage for Overview, Runs, Multi Logs, Inbox.
- [ ] Loading/poll-refresh continuity proof (no blank/dead frame).
- [ ] Error state coverage for render-level and data-level errors.
- [ ] Compare-mode readability proof for multi-log cockpit.

## Suggested evidence workflow

1. Capture legacy and superdash frames for each target id.
2. Attach file paths next to checklist items in review notes.
3. Mark item complete only when both variants present and visually acceptable.
4. Any mismatch: open task with screenshot diff and impacted target id.

## Exit criteria for `forge-qst`

- [ ] Checklist adopted as program gate for `prj-d9j8dpeh`.
- [ ] Target IDs stable and referenced by snapshot-test task (`forge-9r4`).
- [ ] No pane/state/interaction category left untracked.
