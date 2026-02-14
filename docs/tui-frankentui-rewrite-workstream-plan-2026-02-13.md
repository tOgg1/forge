# Forge TUI FrankenTUI Rewrite Workstream Plan (2026-02-13)

Status: active; WS-3/WS-4 largely shipped; WS-5 cutover gates in progress  
Master plan: `docs/tui-frankentui-full-rewrite-plan-2026-02-13.md`
Bootstrap details: `docs/tui-frankentui-unified-bootstrap-plan-2026-02-13.md`

## Cutover changelog (2026-02-13)

Delivered slices:

- `forge-way` command surface first-time flow: onboarding overlay + contextual footer hints + command palette flow.
- `forge-hgn` runs pane rewrite: table selection drives sticky output context.
- `forge-ke7` inbox pane rewrite: subject-first list, compact IDs, unread/ack badges, detail pane.
- `forge-b2q` notification bus/toast queue wiring: bounded queue with shell-level status suffix/error rendering.
- `forge-jd2` multi-logs rewrite: responsive health wall + snapshot parity refresh.

Cutover-adjacent status:

- `forge-kyn` snapshot gate refresh (`80x24`, `120x40`, `200x50`): in progress.
- `forge-hza` docs/runbook/changelog updates: in progress.
- `forge-brp` legacy rendering dead-code removal: pending bake-window completion.

## Workstreams

### WS-0 Safety foundation

- Scope:
  - Replace byte-slice truncation in `crates/forge-tui/src/app.rs`.
  - Add Unicode truncation regression tests.
- Deliverable:
  - shared safe truncate helper + green tests.
- Exit:
  - no `[..width]` slicing in app shell render paths.

### WS-1 Runtime root

- Scope:
  - `crates/forge-tui/src/bin/forge-tui.rs` interactive path.
  - remove implicit snapshot fallback from interactive error path.
  - explicit dev fallback env flag only.
- Deliverable:
  - single-root runtime behavior in normal use.
- Exit:
  - interactive mode never silently drops to snapshot renderer.

### WS-2 Adapter upstream bridge

- Scope:
  - enable upstream-backed implementation surface in `forge-ftui-adapter`.
  - bridge primitives: `Table`, `StatusLine`, `Badge`, `Flex`.
  - adapter translation parity tests.
- Deliverable:
  - stable adapter APIs backed by upstream widgets/layout.
- Exit:
  - tests prove adapter frame parity expectations.

### WS-3 Shell rewrite

- Scope:
  - tab rail, footer hints, status strip.
  - key discoverability contract.
- Deliverable:
  - panel-first shell in rewritten runtime path.
- Exit:
  - no legacy debug wall hints, no static config noise.

### WS-4 Pane rewrites

- Scope:
  - overview, runs, inbox, multi-logs, logs.
- Deliverable:
  - each pane uses upstream-backed adapter primitives.
- Exit:
  - no placeholder logs pane; no bracket-tax rows in runs/inbox.

### WS-5 Gates + cutover

- Scope:
  - snapshot updates, flow checks, flip default path.
  - remove dead legacy paths.
- Deliverable:
  - upstream path default, legacy removed or dev-only.
- Exit:
  - all gates green; docs updated.

## Dependency order

1. `WS-0` and `WS-1` start first.
2. `WS-2` starts in parallel with `WS-1`.
3. `WS-3` depends on `WS-2` minimum APIs.
4. `WS-4` depends on `WS-2` and partially on `WS-3`.
5. `WS-5` after all rewrite streams land.

## Suggested commit slices

1. `fix(tui): replace byte-slice truncation with safe helper`
2. `refactor(tui): remove implicit interactive snapshot fallback`
3. `feat(adapter): bridge upstream table/statusline/badge/flex primitives`
4. `feat(tui): rewrite shell on upstream-backed adapter primitives`
5. `feat(tui): rewrite overview pane`
6. `feat(tui): rewrite runs pane with table`
7. `feat(tui): rewrite inbox pane with list/badge`
8. `feat(tui): rewrite multi-logs pane layout`
9. `feat(tui): replace logs placeholder with real viewer pane`
10. `chore(tui): flip upstream path default and remove legacy dead path`

## Open assignment fields

- Owner per workstream: `TBD`
- ETA per workstream: `TBD`
- Blocking risks:
  - adapter cell translation mismatches
  - runtime startup regressions on low-capability terminals
