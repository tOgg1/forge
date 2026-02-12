# TUI-403 blocker graph and dependency bottleneck view

Task: `forge-318`
Status: delivered

## Scope

- Build a canonical dependency graph model from task relationship samples.
- Rank blocker bottlenecks by impact (direct + transitive blocked tasks).
- Attach actionable drill-down links (`sv task show <id> --json`) for operator follow-up.

## Model

- Input sample:
  - `TaskDependencySample` (`task_id`, `title`, `status`, `blocked_by`, `blocks`)
- Output view:
  - `BlockerGraphView`
  - `BlockerGraphNode`
  - `DependencyEdge`
  - `BottleneckView`
  - `ActionableTaskLink`

## Derivation rules

- Build edges from both `blocked_by` and `blocks`; dedupe deterministically.
- Backfill placeholder nodes for referenced tasks missing from current snapshot.
- Compute impact metrics per node:
  - `direct_blocked_count`
  - `transitive_blocked_count`
  - `impact_score = direct + transitive`
- Classify actionable tasks:
  - non-terminal status
  - all blockers terminal
- For each bottleneck, surface drill-down links to actionable tasks:
  - bottleneck itself if actionable
  - else actionable upstream blockers
  - else actionable downstream tasks

## Implementation

- New module: `crates/forge-tui/src/blocker_graph.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
