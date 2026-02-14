# TUI live layout inspector + perf HUD (forge-6ef)

Date: 2026-02-13
Task: forge-6ef

## What changed
- Added new module `layout_perf_hud` with:
  - `LayoutInspectorSnapshot` for live tab/mode/layout/focus graph state.
  - `FramePerfHud` ring buffer for frame samples.
  - `FrameBudget`, `FramePerfSample`, `FramePerfSummary`.
  - `summarize_frame_perf(...)` and `render_layout_perf_hud_lines(...)`.
- Added `App::layout_perf_hud_snapshot()` to expose live app state for inspector/HUD rendering.
- Exported module via `crates/forge-tui/src/lib.rs`.

## Contracts implemented
- Layout inspector reports:
  - frame + content viewport geometry
  - requested vs effective layout
  - density/focus mode
  - split focus graph path and active node
- Perf HUD reports:
  - budget target (`ms` + `fps`)
  - latest frame/layout/render timings
  - avg/p50/p95/worst frame timings
  - dropped-frame count and budget breach state

## Regression coverage
- `layout_perf_hud::tests::perf_hud_ring_buffer_caps_samples`
- `layout_perf_hud::tests::summarize_frame_perf_reports_percentiles_and_budget_breaches`
- `layout_perf_hud::tests::render_lines_include_focus_graph_and_perf_state`
- `app::tests::layout_perf_hud_snapshot_reflects_focus_and_layout_state`

## Validation
- `cargo test -p forge-tui layout_perf_hud`
- `cargo build -p forge-tui`
