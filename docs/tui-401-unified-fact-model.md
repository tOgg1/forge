# TUI-401 unified fact model for runs, tasks, queues, and agents

Task: `forge-8h3`
Status: delivered

## Schema

- Source repository records:
  - `RunSourceRecord`
  - `TaskSourceRecord`
  - `QueueSourceRecord`
  - `AgentSourceRecord`
  - grouped under `SourceRepositories`
- Unified fact entities:
  - `RunFact`
  - `TaskFact`
  - `QueueFact`
  - `AgentFact`
  - grouped under `UnifiedFactModel`
- Derived totals:
  - `FactTotals` (`runs`, `tasks`, `pending_tasks`, `in_progress_tasks`, `active_agents`)

## Derivation rules

- Normalize status values to stable lowercase vocabulary.
- Derive pending/in-progress queue counts from task facts.
- Backfill queue facts for loops that have task facts but missing queue rows.
- Sort all fact entities deterministically for stable TUI rendering and snapshots.

## Consistency checks (against source repositories)

- Duplicate ID detection:
  - run IDs
  - task IDs
  - agent IDs
- Queue count mismatch detection:
  - repository queue counts vs task-derived queue counts
- Referential integrity checks:
  - orphan run loop references
  - orphan agent loop references
  - missing task assignee agent references

## Implementation

- New module: `crates/forge-tui/src/analytics_fact_model.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
