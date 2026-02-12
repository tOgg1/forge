# TUI-605 activity stream by agent, repo, and task

Task: `forge-vz1`  
Status: delivered

## Scope

- Provide a real-time-ready activity stream model for collaboration actions.
- Support filters by `agent`, `repo`, `task`, `kind`, and free-text query.
- Attach jump links from stream rows to task and logs contexts.

## Implementation

- New module: `crates/forge-tui/src/activity_stream.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core model:

- `ActivityEvent`: normalized event payload (`id/time/kind/summary/agent/repo/task`)
- `ActivityStream`: bounded in-memory stream with deterministic ordering
- `ActivityFilter`: multi-axis stream filter
- `ActivitySnapshot`: filtered rows + counts (`total/matched/dropped`)
- `ActivityRow`: pane-friendly row with jump-link metadata

Core API:

- `push(...)` validates + normalizes incoming events and keeps newest-first window
- `snapshot(...)` returns filtered stream rows with default limit behavior
- `tail_since(...)` returns only newer events for live-update loops

Jump links:

- `task:<task_id>`
- `logs:task:<task_id>`
- `logs:repo:<repo>:agent:<agent>`
- `logs:repo:<repo>`
- `logs:agent:<agent>`

## Regression tests

Added tests in `crates/forge-tui/src/activity_stream.rs` for:

- required-field validation
- deterministic ordering and bounded pruning behavior
- multi-filter matching (`agent/repo/task/kind/text`)
- jump-link emission
- live-tail (`tail_since`) behavior

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
