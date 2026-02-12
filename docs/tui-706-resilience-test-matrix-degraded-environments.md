# TUI-706 resilience test matrix for degraded environments

Task: `forge-d1j`  
Status: delivered

## Scope

- Added deterministic resilience matrix coverage for degraded runtime conditions:
  - missing profiles
  - DB lock contention
  - partial data
  - network interruption
- Added explicit expected-behavior and operator-action guidance per scenario.
- Added severity-based overall status and deterministic report ordering.

## Matrix behavior

- Always emits all four scenario rows.
- Classifies row status as `healthy`, `degraded`, or `blocked`.
- Computes:
  - `overall_status`
  - `degraded_count`
  - `blocked_count`
- Sorts rows by severity first (`blocked > degraded > healthy`) for operator triage.

## Implementation

- New module: `crates/forge-tui/src/resilience_matrix.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Regression tests

- Added coverage for:
  - missing profile blocking behavior
  - DB lock contention degraded behavior
  - partial-data severity thresholds
  - network interruption degraded-vs-blocked staleness gate
  - combined scenario counts + deterministic ordering

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
