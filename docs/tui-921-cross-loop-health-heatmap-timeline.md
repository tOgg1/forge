# TUI-921 cross-loop health heatmap timeline

Task: `forge-h84`  
Status: delivered

## Scope

- Cross-loop timeline heatmap for state and error bursts.
- Deterministic risk ordering across loops.
- Compact text render lines for panel embedding.

## Implementation

- New module: `crates/forge-tui/src/health_heatmap_timeline.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core API:

- `build_cross_loop_health_timeline(inputs, max_buckets)`
- `render_cross_loop_heatmap_lines(timeline, width, max_rows)`

Core model:

- `LoopHealthBucket`
- `LoopHealthTimelineInput`
- `HeatCellSeverity`
- `LoopHealthHeatmapRow`
- `HealthTimelineSummary`
- `CrossLoopHealthTimeline`

Behavior:

- Uses state/error/queue/stall signals to classify each bucket:
  - `.` healthy
  - `:` degraded
  - `!` warning
  - `X` critical
  - `o` offline
- Applies tail-window truncation (`max_buckets`, default 24).
- Ranks loops by critical/warning/degraded pressure and queue peak.

## Regression tests

Added in `crates/forge-tui/src/health_heatmap_timeline.rs`:

- ranked cross-loop severity ordering
- tail-window truncation behavior
- offline glyph mapping
- rendered summary/legend/row output
- invalid-row filtering

## Validation

- `cargo test -p forge-tui health_heatmap_timeline -- --nocapture`
- `cargo build -p forge-tui`
