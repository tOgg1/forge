# Blame Timeline (forge-hj8)

Date: 2026-02-13
Task: `forge-hj8`

## Scope implemented

Added `crates/forge-tui/src/blame_timeline.rs` and exported it from `crates/forge-tui/src/lib.rs`.

Model capabilities:

- `ChangeOutcome`: `active`, `reverted`, `partial-revert` status labels.
- `BlameTimelineEntry`: per-change metadata for file region, agent, task, intent, confidence, commit, and optional revert metadata.
- `FileBlameTimeline`:
  - `record_change(...)` validation + deterministic ordering
  - single-file scoping guard
  - revert metadata requirements for reverted entries
  - `entries_touching_line(line)` region lookup for inline blame views
  - `agent_summary()` aggregation by agent and outcome buckets
  - `render_rows(width, max_rows)` deterministic text rows for TUI pane/snapshot wiring

## Test coverage added

In `blame_timeline::tests`:

- invalid region/confidence validation
- reverted-entry metadata requirements
- deterministic sort order by time and region
- line-region filtering behavior
- per-agent outcome-bucket summary counts
- rendered row snapshot determinism with revert annotations

## Validation

Executed:

- `cargo fmt --all -- crates/forge-tui/src/blame_timeline.rs crates/forge-tui/src/lib.rs`
- `cargo test -p forge-tui blame_timeline::tests:: -- --nocapture`
