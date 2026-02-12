# TUI-207 activity heatmap and sparklines

Task: `forge-bc7`  
Status: delivered

## Scope

- Added compact trend visuals per loop for:
  - run rate
  - error rate
  - duration trend
  - latency trend
- Added compact activity heatmap row with alert emphasis for error spikes.

## Implementation

- New module: `crates/forge-tui/src/activity_heatmap.rs`
- New contracts:
  - `LoopTrendBucket`
  - `LoopTrendInput`
  - `LoopTrendSummary`
  - `LoopTrendVisual`
- New API:
  - `build_loop_activity_trends(...)`
- Exported from crate root in `crates/forge-tui/src/lib.rs`.

## Derivation behavior

- Buckets are sorted by timestamp and tail-window limited (`max_buckets`, default 24).
- Sparklines are generated for run/error/duration/latency using deterministic ASCII levels.
- Heatmap glyphs combine activity/latency intensity and promote error spikes:
  - `!` for medium error bursts
  - `X` for severe error bursts
- Loop ranking is deterministic: highest error-rate first, then error volume, then loop id.

## Regression tests

- Visual + summary derivation per loop.
- Tail-window bucket truncation behavior.
- Severity-based ranking order.
- Empty-id / empty-bucket skipping.
- Error-spike heatmap glyph behavior.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
