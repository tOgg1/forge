# Fmail Correspondence + Brainstorm Pass 3 (2026-02-13)

Topic: `tui-visual-polish`
Window reviewed: latest lock-round through ~19:19 local thread time.

## Current consensus (unchanged)
- Rewrite direction: full FrankenTUI path.
- Guardrails: WI-0 first, parity gates (`80x24`,`120x40`,`200x50`), no implicit fallback.
- P0 unchanged:
  1) Runs triage table + sticky output
  2) Incident-first inbox queue
  3) Multi-loop health wall
  4) Command surface bar
  5) Modal action rail

## New deltas from this pass

1. Runtime architecture alignment strengthened:
- Multiple agents now explicitly favor Option A: direct `Model` implementation in `forge-tui`.
- Adapter reframed as style/theme/widget-kit layer; not runtime mediator.

2. Toolchain blocker refined:
- `tui-polish-lead` reports pinned rev compiles on stable for widget-only surface (`default-features = false`).
- Runtime path (`features = ["runtime"]` and `runtime+crossterm`) still unverified; this remains day-0 check.
- `trmd-opus` marks nightly requirement as UNVERIFIED until runtime feature test is run.

3. Ranked feature additions beyond P0/P1:
- P1-fast candidates: cross-tab deep links, evidence hotkeys, breadcrumb return, loop lifecycle timeline.
- Additional strong candidates: Next-Action Panel (overview), Claim Timeline rail (inbox).
- P2/P3 buckets now clearer: undo/redo, state persistence, error boundaries, clickable links/hit-grid polish, incident playback, semantic map.

## New concrete asks from agents

1. Verify runtime toolchain now:
- compile `ftui` with `runtime`
- compile `ftui` with `runtime,crossterm`
- decide stable vs nightly scope based on result

2. Preserve seams during WS work:
- log-source abstraction (parsed / diff / PTY)
- styled-span pipeline for markdown/syntax
- app-wide notification bus
- navigation stack for deep links + breadcrumb backtrack

## Suggested immediate lock actions

1. Run runtime-feature compile probes (stable toolchain) before WS-1 starts.
2. Confirm Option A architecture in workstream docs.
3. Keep P0 frozen; queue P1-fast as post-cutover batch.
