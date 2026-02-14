# Forge TUI World-Class + FrankenTUI Investigation (2026-02-13)

## Goal

- Make `forge-tui` truly world-class.
- Make primary flows obvious in <60s for first-time operator.
- Go panel-first.
- Use FrankenTUI capabilities directly, not a thin "ASCII app with borders."

## What is already strong

- Program roadmap broad + deep (`docs/tui-next-roadmap-2026-02-10.md`).
- Visual polish pass already scoped (`docs/tui-visual-polish-plan.md`).
- Snapshot gate exists for 80x24 / 120x40 / 200x50 (`docs/tui-909-layout-snapshot-breakpoint-gate.md`).
- Parity checklist exists for all major panes (`docs/tui-907-visual-parity-checklist-target-screenshots.md`).

## Current gaps (high impact)

1. FrankenTUI mostly bypassed at runtime.
- Upstream crate is optional (`crates/forge-ftui-adapter/Cargo.toml:15`).
- Adapter exports local render API, custom panel draw, custom frame (`crates/forge-ftui-adapter/src/lib.rs:14`, `crates/forge-ftui-adapter/src/lib.rs:495`).
- Result: not leveraging core ftui widgets/runtime stack.

2. Runtime root split still present.
- Interactive runtime falls back to snapshot renderer on error (`crates/forge-tui/src/bin/forge-tui.rs:79`).
- Conflicts with single-root intent documented in `docs/tui-912-frankentui-shell-single-root-bootstrap.md`.

3. Shell still noisy/debug-like.
- Header carries static config metadata each frame (`crates/forge-tui/src/app.rs:3360`).
- Footer global hint wall, low context sensitivity (`crates/forge-tui/src/app.rs:3305`).
- Byte-slice truncation risk with Unicode (`crates/forge-tui/src/app.rs:3369`, `crates/forge-tui/src/app.rs:3313`).

4. Panel content still too "operator-debug" in key tabs.
- Overview run snapshot uses key-value debug line (`crates/forge-tui/src/overview_tab.rs:400`).
- Runs timeline still lane/tree style + bracket badges (`crates/forge-tui/src/runs_tab.rs:341`).
- Multi-logs header/meta carries dense control strings (`crates/forge-tui/src/multi_logs.rs:237`, `crates/forge-tui/src/multi_logs.rs:508`).
- Inbox thread rows prioritize IDs + counters over subject clarity (`crates/forge-tui/src/app.rs:3623`).

5. Advanced TUI modules under-used in primary shell.
- Main app rendering path wired mainly to overview/runs/multi/inbox and few helpers (`crates/forge-tui/src/app.rs:3221`).
- Many "next-gen" modules exist but not surfaced in default cockpit flow (`crates/forge-tui/src/lib.rs:8`).

## FrankenTUI capabilities to use now (pinned rev)

Source: pinned checkout `23429fac0e739635c7b8e0b995bde09401ff6ea0`.

1. Runtime/screen modes.
- Inline, InlineAuto, AltScreen + top/bottom anchors (`ftui-runtime/src/terminal_writer.rs:237`, `ftui-runtime/src/terminal_writer.rs:259`).

2. Deterministic rendering + partial updates.
- Diff strategy config + dirty-row/dirty-span/tile options (`ftui-runtime/src/terminal_writer.rs:330`).
- Buffer diff and one-writer model (`README.md`, `docs/one-writer-rule.md`).

3. Observability for UI correctness/perf.
- Evidence sink JSONL (`ftui-runtime/src/evidence_sink.rs:37`).
- Render trace recorder (`ftui-runtime/src/render_trace.rs:28`).
- Budget/degradation hooks in runtime builder (`ftui-runtime/src/program.rs:3197`).

4. Layout primitives for real panel systems.
- Flex + Grid + responsive breakpoints (`ftui-layout/src/lib.rs:1`).

5. Widget surface already rich.
- Panel, table, list, tree, log viewer, command palette, modals, notification queue, focus manager, virtualized list (`ftui-widgets/src/lib.rs:126`).
- Modal stack and modal animation included (`ftui-widgets/src/modal/stack.rs:1`, `ftui-widgets/src/modal/animation.rs:1`).

6. Large-data UX.
- Virtualization path for 100k+ item views (`ftui-widgets/src/virtualized.rs:1`).
- LogViewer built for streaming + search/filter/wrap (`ftui-widgets/src/log_viewer.rs:1`).

## World-class target UX (panel-first)

## Shell

- Top: compact status strip (fleet + active context + transient mode/follow only).
- Mid: tab-specific panel layout (not one monolithic canvas).
- Bottom: context-sensitive action hints (max 6-8 hints).
- Transient layer: command/message palettes, modals, toasts.

## Per-tab panel contracts

1. Overview
- Hero panel (fleet health).
- Selected loop summary panel (2-column grouped fields).
- Run snapshot panel (human labels, no `key=value` dump).
- Next-action panel ("what to do now").

2. Runs
- Table panel (ID/status/exit/duration/profile).
- Output panel (selected run, sticky title + scroll state).
- Selection row full-width highlight.

3. Multi Logs
- Compact global meta strip (layout + page + layer only).
- Virtualized mini-panels with sticky per-loop header.
- Empty mini-pane guidance, no dead space.

4. Inbox
- Subject-first thread list.
- Detail panel with first message preview, not hint text.
- Claim timeline panel as dedicated rail with conflict emphasis.

## Recommended implementation sequence

1. Phase A: Safety + shell cleanup (fast)
- Add grapheme-safe trimming helper; replace byte slicing in header/footer/tab text.
- Keep transient mode/follow in header; remove static config metadata.
- Make footer hints tab-contextual.

2. Phase B: True FrankenTUI runtime adoption
- Enable upstream adapter path by default.
- Migrate render loop to ftui `AppBuilder` + `ProgramConfig`.
- Keep fallback only behind explicit dev flag, not implicit runtime path.

3. Phase C: Panel system rewrite on ftui widgets
- Replace custom panel draw + row composition with ftui `Panel`, `Table`, `List`, `LogViewer`.
- Add focus graph/traps for keyboard traversal across panels + modals.
- Use `ModalStack` + `NotificationQueue` for wizard/confirm/status flows.

4. Phase D: Performance + reliability gates
- Turn on evidence sink in CI smoke jobs.
- Capture render traces for golden interactions.
- Gate by frame budget + diff churn for 80x24 and 120x40.

## Acceptance gates for "world-class + easy"

1. First-time operator flow:
- Open TUI.
- Identify failing loop.
- Jump to run/log.
- Send action/message.
- Return to overview.
- <= 60s without reading docs.

2. Navigation quality:
- 100% keyboard; no dead-end focus.
- Every tab has <= 8 contextual hints.

3. Visual quality:
- No debug-like `key=value` walls in primary panes.
- No empty 20+ row dead panels when data sparse.

4. Runtime quality:
- No implicit renderer fallback in normal interactive path.
- Resize storms: no flicker, no lockups.
- Trace/evidence produced for perf regressions.

## Immediate next work items

1. Fix Unicode-safe truncation in shell + add regression tests.
2. Implement polished runs table + row background highlight (fixed-width first).
3. Simplify multi-logs header/meta text and move key hints to footer.
4. Rework inbox thread row format (subject-first, compact ID suffix, badges).
5. Add feature-flagged ftui widget-backed shell prototype behind one switch.
6. Decide cut date for removing implicit interactive snapshot fallback.

## External inputs used

- FrankenTUI web/docs + GitHub examples/readme.
- fmail design feedback from `tui-polish-lead`:
  - fixed-width runs table first
  - Unicode-safe truncation prerequisite
  - keep mode/follow indicators; remove only static config metadata
  - keep compact inbox message IDs
