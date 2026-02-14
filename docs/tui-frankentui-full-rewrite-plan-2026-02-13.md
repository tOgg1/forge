# Forge TUI Full FrankenTUI Rewrite Plan (2026-02-13)

Status: draft  
Decision source: owner directive via `tui-visual-polish` fmail thread (`20260213-183828-0000`).
Bootstrap contract: `docs/tui-frankentui-unified-bootstrap-plan-2026-02-13.md`

## Decision

- Do full rewrite now on true FrankenTUI path.
- Treat current renderer path as legacy bootstrap, not target architecture.
- Per-work-item commits mandatory.

## Goals

1. Single runtime root on FrankenTUI.
2. No implicit interactive snapshot fallback.
3. Panel-first shell and panes rebuilt on real upstream widgets/layout.
4. Keep operator parity on critical workflows during transition.

## Non-goals

1. Preserve current custom rendering internals.
2. Ship mixed long-term dual-render architecture.

## Target architecture

1. `forge-tui` app logic remains domain owner.
2. `forge-ftui-adapter` becomes strict bridge to upstream `ftui`.
3. Upstream path enabled by default after parity gates.
4. Legacy snapshot renderer only behind explicit dev/diagnostic flag.

## Rewrite phases

1. Phase 0: freeze + safety
- Freeze new polish-only edits on legacy path.
- Add width-safe truncation utility and tests (Unicode-safe).
- Keep existing snapshot tests green before large refactor.

2. Phase 1: runtime cut
- Wire interactive entrypoint to FrankenTUI runtime path.
- Remove implicit fallback in normal interactive execution.
- Add explicit `FORGE_TUI_DEV_SNAPSHOT_FALLBACK=1` escape hatch.

3. Phase 2: adapter upstream bridge
- Enable upstream feature path in adapter implementation.
- Add bridge surface for minimum widget set:
  - `Table`
  - `StatusLine`
  - `Badge`
  - `Flex` layout primitives
- Add cell/style parity tests (`ftui` buffer -> adapter frame model).

4. Phase 3: shell rewrite
- Rebuild tab rail, footer hints, status strip on upstream-backed primitives.
- Keep key discoverability contract (`1..5` tab affordance explicit).

5. Phase 4: pane rewrites
- Overview on panel + flex composition.
- Runs on table + output viewer composition.
- Inbox on list/panel/badge composition.
- Multi-logs on responsive grid/flex composition.
- Logs on real viewer flow (no placeholder path).

6. Phase 5: cutover + cleanup
- Flip upstream path to default.
- Lock parity and workflow gates.
- Remove dead legacy rendering code paths.

## Acceptance gates

1. Build/test quality
- `cargo fmt --check`
- `cargo clippy -p forge-tui --all-targets -- -D warnings`
- `cargo test -p forge-ftui-adapter -p forge-tui`

2. Snapshot parity
- `80x24`, `120x40`, `200x50` snapshots updated and reviewed.
- No truncation panics with Unicode-heavy labels.

3. Workflow gate
- First-time operator flow end-to-end in <=60s (manual runbook script).

4. Runtime gate
- No implicit interactive fallback.
- Explicit dev fallback documented and tested.

## Commit policy

- One commit per work item.
- Conventional commit prefixes.
- No squashed mega-commit for rewrite.
