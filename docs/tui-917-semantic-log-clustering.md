# tui-917: Semantic log clustering

## Scope
- Task: `forge-n2z`
- Objective: deduplicate similar error lines across loop streams and surface unique errors.

## What landed
- New module: `crates/forge-tui/src/semantic_log_clustering.rs`
  - `cluster_semantic_errors_by_loop(...)`
  - `compact_cluster_summary(...)`
- New data model:
  - `SemanticErrorCluster`
  - `ErrorInstance`
- Clustering behavior:
  - keeps only error-like lines (`stderr` lane or error keywords)
  - strips anomaly marker prefix (`! [ANOM:...]`) before grouping
  - normalizes IDs and numeric tokens (`30s`, `31s` -> `#s`) to group semantically similar lines
  - ranks clusters by occurrence count, then loop spread

## UI surfacing
- Multi-logs subheader now includes compact cluster summary:
  - `clusters:none` or `clusters:N top:<representative> xOCC/LOOPSl`
- Wiring in: `crates/forge-tui/src/multi_logs.rs`

## Tests
- Added semantic clustering unit tests:
  - cross-loop grouping for timeout variants
  - non-error line exclusion
  - anomaly-prefix stripping
  - compact summary shape
- Multi-logs subheader regression asserts `clusters:` token is shown.
