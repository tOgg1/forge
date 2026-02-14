# TUI-926 timeline swim lanes

Task: `forge-s63`  
Status: delivered

## Scope

- Concurrent run visualization across loops in horizontal swim lanes.
- Overlap/parallelism visibility for contention detection.
- Keyboard-friendly zoom/pan window model at data-layer level.

## Implementation

- Added module: `crates/forge-tui/src/timeline_swim_lanes.rs`
- Added model:
  - `SwimLaneRunSample`
  - `TimelineSwimLaneConfig`
  - `SwimLaneSegment`, `SwimLane`, `TimelineSwimLaneReport`
- Added planner:
  - `build_timeline_swim_lanes(samples, config)`
  - Windowing:
    - explicit `window_start_ms/window_end_ms`
    - `pan_ms` shift
  - Lane grouping by `loop_id`
  - Segment projection into timeline columns
  - Contention scoring:
    - overlap cell count
    - max parallel runs
    - segment count
  - Sorted lanes by contention; applies `lane_limit`.
- Added renderer:
  - `render_timeline_swim_lanes(report, width, height)`
  - Status markers per segment (`=`, `+`, `x`, `~`, `-`, `#`)
  - Overlap collision marker (`*`)
- Exported module in `crates/forge-tui/src/lib.rs`.

## Validation

- `cargo test -p forge-tui timeline_swim_lanes::tests::`
- `cargo build -p forge-tui`
