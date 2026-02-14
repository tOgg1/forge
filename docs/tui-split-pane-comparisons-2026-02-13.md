# Split Pane Comparisons Baseline (2026-02-13)

Task: `forge-4z7`

## Outcome

Closed by evidence verification: split-pane comparison baseline is already present in `forge-tui`.

## Existing implementation coverage

- Multi Logs side-by-side compare mode:
- Toggle via `C` in `MainTab::MultiLogs` (`App::toggle_multi_compare_mode`).
- Shared/synchronized scrolling behavior via `log_compare::synchronized_windows`.
- Row-level diff hint rendering in compare pane (`multi_logs.rs`).

- Compare model foundation for broader side-by-side use:
- `crates/forge-tui/src/multi_node_compare_split.rs` provides deterministic compare report and panel-render lines for node drift inspection.

## Verified signals

- Compare rendering and interactions in multi logs:
- `multi_logs::tests::compare_mode_toggle_renders_side_by_side_header`
- `multi_logs::tests::compare_mode_scroll_keys_update_shared_scroll`
- `multi_logs::tests::compare_mode_renders_row_level_diff_hints`

- Sync window alignment logic:
- `log_compare::tests::synchronized_windows_prefers_matching_timestamp_anchor`
- `log_compare::tests::synchronized_windows_falls_back_to_ratio_anchor`

## Validation

- `cargo test -p forge-tui compare_mode_ -- --nocapture`
- `cargo test -p forge-tui synchronized_windows_ -- --nocapture`
- `cargo build -p forge-tui`
