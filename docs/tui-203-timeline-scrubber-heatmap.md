# TUI-203 timeline scrubber with density heatmap

Task: `forge-m7a`  
Status: delivered

## Scope

- Add timeline scrubber model for large log streams.
- Add density heatmap indicators for activity + error-heavy buckets.
- Guarantee anchored seek behavior so cursor row stays stable while scrubbing.

## Implementation

- New module: `crates/forge-tui/src/timeline_scrubber.rs`
- Exported in: `crates/forge-tui/src/lib.rs`

Core types:

- `TimedLogLine`: timestamped log line metadata (`line_index`, `is_error`)
- `TimelineBucket`: per-time-slice line/error counts + line-span references
- `TimelineHeatmap`: bucket collection + max counts for density scaling
- `TimelineScrubber`: stateful scrubber with selected bucket tracking
- `CursorAnchor` + `SeekWindow`: anchored navigation contract

Core behavior:

- `build_timeline_heatmap(...)`:
  - O(n) bucket aggregation across timestamps
  - preserves first/last line mapping per bucket
  - supports empty-input bucket scaffolding
- Heatmap rendering:
  - `render_density_line()` produces one glyph per bucket
  - error buckets are emphasized with `!`/`X`
  - `render_error_line()` + `render_selection_line()` expose independent overlays
- Time navigation:
  - `seek_to_ratio(...)` maps scrub ratio -> bucket -> target line
  - empty buckets fallback to nearest non-empty bucket
  - `anchored_seek(...)` keeps target line anchored to a stable viewport row

## Regression tests

Added tests in `crates/forge-tui/src/timeline_scrubber.rs` for:

- bucket aggregation, ranges, and maxima
- heatmap/error/selection rendering output
- ratio clamping and bucket mapping
- anchored seek stability + head/tail clamping
- empty-bucket fallback seek behavior
- large-log scrub validity (`200k` lines)

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
