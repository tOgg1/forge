# tui-918: Incident correlation engine

## Scope
- Task: `forge-0sg`
- Goal: auto-detect likely shared root-cause incidents across loop failures and surface a correlation card/summary.

## Implementation
- Reused existing correlation core in `crates/forge-tui/src/error_correlation.rs`.
- Wired multi-loop signal extraction + summary rendering in `crates/forge-tui/src/multi_logs.rs`:
  - `correlation_summary_for_targets(...)`
  - `is_error_candidate(...)`
  - `strip_anomaly_prefix(...)`
- Multi Logs subheader now shows both:
  - semantic clusters (`clusters:*`)
  - incident correlation (`corr:*`)

## Correlation summary format
- `corr:none` when no meaningful cross-loop incident detected.
- `corr:<loops>l/<events>e c<confidence>% <representative>` when detected.
  - example: `corr:3l/7e c86% timeout waiting for daemon`.

## Inputs used for correlation
- Per-loop recent tail lines (last 32) that match error heuristics.
- Loop-level `last_error` when present.
- Existing anomaly prefixes (`! [ANOM:...]`) are stripped before correlation.

## Tests
- Added regression test in `crates/forge-tui/src/multi_logs.rs`:
  - `render_multi_logs_pane_subheader_shows_cross_loop_correlation`
- Existing subheader test now asserts `corr:` token presence.
