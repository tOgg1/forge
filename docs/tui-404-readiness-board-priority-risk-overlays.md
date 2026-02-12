# TUI-404 readiness board with priority and risk overlays

Task: `forge-1fx`
Status: delivered

## Scope

- Build readiness board model for task cockpit views.
- Combine readiness state, priority overlay, stale-risk signal, and ownership-gap signal.
- Support scoped filters for project and epic slices.

## Model

- Input:
  - `ReadinessTaskSample` (`task_id`, `title`, `status`, `priority`, `project_id`, `epic_id`, `owner`, `updated_at_epoch_s`, `blocked_by`)
  - `ReadinessBoardFilter` (`project_ids`, `epic_ids`, `include_terminal`)
- Output:
  - `ReadinessBoardView`
  - `ReadinessBoardRow`
  - `ReadinessBoardSummary`
  - `PriorityOverlayCount`

## Derivation rules

- Apply case-insensitive project/epic filters before rendering.
- Readiness labels:
  - `ready` for open/ready/pending/queued without blockers
  - `active` for in-progress/running statuses
  - `blocked` for blocked statuses or non-empty blocker list
  - `terminal` for closed/completed/failed states
- Risk overlays per row:
  - `priority:<Pn>`
  - `risk:blocked` when blocked
  - `risk:stale` when `now - updated_at >= stale_after_secs` (default `3600`)
  - `risk:owner-gap` when non-terminal row has no owner
- Deterministic ordering:
  - stale risk first
  - ownership gaps next
  - blocked rows next
  - then priority rank (`P0..P3`)
  - then readiness score and task id

## Implementation

- New module: `crates/forge-tui/src/readiness_board.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
