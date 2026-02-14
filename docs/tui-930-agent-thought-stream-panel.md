# TUI-930: Agent Thought Stream Panel

Task: `forge-sn1`

## Scope
- Add a focused-loop thought-stream model for agent reasoning flow.
- Surface decision-tree context: considered/rejected/selected/invoked tool paths.
- Provide collapsed + expanded panel rendering for operator diagnostics.

## Implementation
- Added `crates/forge-tui/src/agent_thought_stream.rs` with:
  - `ThoughtEventKind`, `ThoughtEvent`, `ThoughtBranchSummary`, `ThoughtStreamReport`.
  - `build_agent_thought_stream(...)`:
    - filters by focused loop + agent
    - normalizes/sorts event stream
    - computes branch summaries and selected branch
    - applies stuck heuristics (candidate churn / context-only loops)
  - `render_agent_thought_stream_panel(...)`:
    - compact header/status lines
    - collapsed mode: latest decision context
    - expanded mode: branch summary + recent event rows
- Exported module in `crates/forge-tui/src/lib.rs`.

## Regression Tests
- `agent_thought_stream::tests::build_filters_and_sorts_events_by_focus`
- `agent_thought_stream::tests::build_marks_selected_branch_and_branch_counts`
- `agent_thought_stream::tests::stuck_heuristic_flags_candidate_churn_without_execution`
- `agent_thought_stream::tests::render_collapsed_shows_latest_event_and_status`
- `agent_thought_stream::tests::render_expanded_shows_branch_and_recent_sections`
- `agent_thought_stream::tests::render_empty_report_shows_no_events_message`

## Validation
- `cargo fmt --package forge-tui`
- `cargo test -p forge-tui agent_thought_stream::tests:: -- --nocapture` (pass)
- `cargo build -p forge-tui` (blocked by unrelated concurrent `forge-cli/src/workflow.rs` compile errors)
