# TUI-932: Smart Context Panel (Auto-Surfaced Intel)

Task: `forge-m4t`

## Scope
- Auto-surface contextual intel for a focused loop.
- Rank related loops, fmail threads, commits, sibling failures, and dependency-neighborhood context.
- Provide deterministic sidebar render lines for operator consumption.

## Implementation
- Added `crates/forge-tui/src/smart_context_panel.rs`.
- Input model:
  - `ContextLoopSample`, `ContextFmailThread`, `ContextCommit`, `ContextFailureSample`, `ContextDependencyLink`, `SmartContextPanelInput`.
- Output model:
  - `SmartContextPanelReport` with ranked lists:
    - `related_loops`
    - `fmail_mentions`
    - `relevant_commits`
    - `sibling_failures`
    - `graph` (`upstream`, `downstream`, `siblings`)
- Core logic:
  - semantic token overlap scoring (loop summary/files/crates vs thread/commit/failure context)
  - direct loop mention bonuses
  - dependency-neighbor bonus
  - recency bucket bonus
  - sibling-failure signature overlap scoring
- Rendering:
  - `render_smart_context_panel_lines(...)` with compact sectioned output.
- Exported via `crates/forge-tui/src/lib.rs`.

## Regression Tests
- `smart_context_panel::tests::related_loops_prioritize_semantic_and_dependency_overlap`
- `smart_context_panel::tests::fmail_mentions_prioritize_direct_loop_reference`
- `smart_context_panel::tests::commit_and_failure_context_are_ranked`
- `smart_context_panel::tests::graph_neighborhood_tracks_upstream_downstream_and_siblings`
- `smart_context_panel::tests::render_contains_all_context_sections`

## Validation
- `cargo fmt --package forge-tui`
- `cargo test -p forge-tui smart_context_panel::tests:: -- --nocapture`
- `cargo build -p forge-tui`
