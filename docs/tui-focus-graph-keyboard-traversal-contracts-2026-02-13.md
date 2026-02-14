# Focus Graph + Keyboard Traversal Contracts (2026-02-13)

Task: `forge-24f`

## Contract

Main mode uses a two-node focus graph (`left`, `right`) for split-pane tabs.

## Split Tabs

- `Overview`
- `Logs`
- `Runs`
- `Multi Logs`
- `Inbox`

## Traversal Keys

- `Tab`: next focus node (wrap)
- `Shift+Tab`: previous focus node (wrap)
- `Left`: previous focus node (wrap)
- `Right`: next focus node (wrap)

## Guarantees

- No dead-end focus in split-pane main tabs.
- Traversal wraps around the graph.
- Status line reflects active focus side (`left`/`right`).

## Regression Tests

- `tab_and_shift_tab_wrap_focus_graph_in_main_mode`
- `left_right_wrap_focus_graph_without_dead_end`
