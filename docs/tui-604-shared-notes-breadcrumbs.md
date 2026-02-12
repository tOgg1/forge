# TUI-604 shared notes and breadcrumbs per task

Task: `forge-daf`  
Status: delivered

## Scope

- Add lightweight shared notes per task.
- Add timestamped breadcrumb trail with author attribution.
- Provide pane-ready timeline rows for TUI rendering.

## Implementation

- New module: `crates/forge-tui/src/task_notes.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core model:

- `TaskNotesBoard`: in-memory task->thread store
- `TaskNotesThread`: grouped notes + breadcrumbs for one task
- `TaskNoteEntry`: timestamp/author/body
- `TaskBreadcrumb`: timestamp/author/kind/summary/related reference
- `TaskTimelineRow`: merged row type for notes pane

Core API:

- `add_note(...)` with required-field validation
- `add_breadcrumb(...)` with kind + optional related ref
- `timeline_rows(task_id)` merged + deterministic sort
- `render_task_notes_pane(...)` compact pane lines (`timestamp + author + label + text`)

Breadcrumb kinds:

- `note`, `status`, `command`, `handoff`, `risk`

## Regression tests

Added tests in `crates/forge-tui/src/task_notes.rs` for:

- required-field validation for notes/breadcrumbs
- merged timeline ordering across note + breadcrumb events
- related-reference retention
- notes pane rendering with attribution and linkage
- empty-pane hint rendering

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
