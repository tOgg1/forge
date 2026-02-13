# tui-916: Rule-based log anomaly detector

## Scope
- Task: `forge-xfy`
- Goal: highlight unusual log signatures in-stream with deterministic rules.

## Implementation
- Added detector + annotation API in `crates/forge-tui/src/log_pipeline.rs`:
  - `detect_rule_based_anomalies(lines)`
  - `annotate_lines_with_anomaly_markers(lines, anomalies)`
- Wired into log rendering paths:
  - `crates/forge-tui/src/multi_logs.rs` (`render_log_block`)
  - `crates/forge-tui/src/interactive_runtime.rs` (live log layer and run-output path)

## Detection rules
- `PANIC`: `panic`, `fatal`, `segmentation fault`, `assertion failed`
- `OOM`: `out of memory`, `oom`, `cannot allocate memory`, `no space left on device`, `killed process`
- `TIMEOUT`: `timed out`, `timeout`, `deadline exceeded`, `connection reset/refused`
- `EXIT`: non-zero value parsed from `exit code` or `exit status`
- `REPEATxN`: normalized error-like signature appears at least 3 times in window

## Marker format
- Prefix per matched line:
  - `! [ANOM:PANIC] ...`
  - `! [ANOM:TIMEOUT,REPEATx3] ...`
- Existing already-prefixed lines are left unchanged (idempotent pass).

## Tests
- `crates/forge-tui/src/log_pipeline.rs`
  - detection: repeat/signature kinds, zero-exit ignore, annotation formatting
- `crates/forge-tui/src/multi_logs.rs`
  - marker prefix regression in `render_log_block`
