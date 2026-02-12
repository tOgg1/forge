# TUI-405 next-best-task recommendation engine

Task: `forge-mdc`  
Status: delivered

## Scope

- Recommend candidate tasks per operator context.
- Use weighted signals:
  - priority
  - readiness
  - dependency/blocked state
  - ownership
  - optional project/epic focus

## Implementation

- New module: `crates/forge-tui/src/task_recommendation.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

### Core API

- `recommend_next_best_tasks(samples, context) -> RecommendationReport`
  - excludes terminal tasks
  - computes deterministic score breakdown per candidate
  - returns reason strings for explainability
  - supports default/explicit recommendation limits
- Score components:
  - `priority_score`
  - `readiness_score`
  - `dependency_score`
  - `ownership_score`
  - `context_score`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
