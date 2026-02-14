# forge-p6c - forge-tui too_many_arguments slice

Date: 2026-02-14
Task: `forge-p6c`
Scope:
- `crates/forge-tui/src/blame_timeline.rs`
- `crates/forge-tui/src/multi_logs.rs`
- `crates/forge-tui/src/overview_tab.rs`
- `crates/forge-tui/src/task_notes.rs`

## Change

- Added targeted `#[allow(clippy::too_many_arguments)]` on four intentionally wide API surfaces:
  - `FileBlameTimeline::record_change`
  - `MultiLogsState::render_compare_logs_pane`
  - `render_overview_paneled_with_options`
  - `OperatorDecisionJournal::record_action`

## Validation

```bash
cargo clippy -p forge-tui --all-targets -- -D warnings -A clippy::expect_used -A clippy::unwrap_used
cargo clippy -p forge-tui --all-targets -- -D clippy::too_many_arguments
cargo test -p forge-tui --lib blame_timeline::tests
cargo test -p forge-tui --lib multi_logs::tests
cargo test -p forge-tui --lib overview_tab::tests
cargo test -p forge-tui --lib task_notes::tests
```

Result:
- Clippy sweep passed for `forge-tui` under current lint profile.
- Focused `-D clippy::too_many_arguments` sweep passed.
- All four focused module test groups passed.
