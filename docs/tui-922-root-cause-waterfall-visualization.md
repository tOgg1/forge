# TUI-922 root cause waterfall visualization

Task: `forge-k96`  
Status: delivered

## Scope

- Waterfall diagram model for causal failure chains.
- Root->terminal path extraction across upstream dependencies.
- Compact timeline row rendering for operator triage.

## Implementation

- New module: `crates/forge-tui/src/root_cause_waterfall.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core API:

- `build_root_cause_waterfall(events, timeline_width)`
- `render_root_cause_waterfall_lines(waterfall, width, max_rows)`

Core model:

- `WaterfallSeverity`
- `WaterfallEvent`
- `WaterfallRow`
- `RootCauseWaterfall`

Behavior:

- Selects terminal failure (latest error/critical event).
- Backtracks upstream dependencies to produce root path.
- Computes hop count and scaled timeline columns.
- Annotates rows as root/terminal/path/failure.

## Regression tests

Added in `crates/forge-tui/src/root_cause_waterfall.rs`:

- 3-hop causal path tracing
- terminal selection behavior
- unknown-upstream filtering
- rendered header/legend/row output
- empty input fallback

## Validation

- `cargo test -p forge-tui root_cause_waterfall -- --nocapture`
- `cargo build -p forge-tui`
