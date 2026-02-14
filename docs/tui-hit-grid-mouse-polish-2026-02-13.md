# Hit-Grid Mouse Polish (2026-02-13)

Task: `forge-j2z`

## Scope

- Add click/drag focus and pane interactions in the Forge TUI.
- Extend mouse event model so app logic can receive coordinates + button/action.

## Changes

- `crates/forge-ftui-adapter/src/lib.rs`
- Expanded canonical mouse model:
- Added `MouseButton` (`Left`, `Right`, `Middle`).
- Added `MouseEventKind` (`Wheel`, `Down`, `Up`, `Drag`, `Move`).
- `MouseEvent` now includes `kind`, `column`, `row`.
- Updated default input translation to map wheel events through `MouseEventKind::Wheel`.

- `crates/forge-tui/src/frankentui_bootstrap.rs`
- Runtime mouse translation now maps upstream crossterm-like events into the richer adapter mouse model.
- Preserves wheel behavior and now forwards left/right/middle down/up/drag/move with coordinates.

- `crates/forge-tui/src/app.rs`
- Added `InputEvent::Mouse` handling in the main update loop.
- Added hit-grid style interactions:
- Tab rail click switches tabs.
- Inbox pane:
- Click/drag in thread list focuses list pane and updates selected thread.
- Click in detail pane focuses detail pane.
- Multi Logs pane:
- Click in matrix cells selects corresponding loop and focuses right pane.
- Wheel in inbox moves thread selection.
- Wheel in logs/runs/multi-logs scrolls log view.
- Added helper methods for content metrics, tab hit-testing, and pane-specific mouse hit handlers.

- `crates/forge-cli/src/workflow.rs`
- Added missing `Arc`/`Mutex` imports and restored missing workflow concurrency fields/constants needed for current branch to compile during `forge-tui` build validation.

## Regression coverage

- `app::tests::mouse_click_tab_rail_switches_tabs`
- `app::tests::mouse_click_inbox_list_selects_thread_and_focuses_list_pane`
- `app::tests::mouse_drag_inbox_list_updates_thread_selection`
- `app::tests::mouse_click_multi_logs_cell_selects_corresponding_loop`
- `app::tests::mouse_wheel_scrolls_inbox_thread_selection`
- `frankentui_bootstrap::tests::translate_runtime_event_maps_key_resize_mouse`
- `forge_ftui_adapter::tests::input_translation_mouse_wheel`

## Validation

- `cargo test -p forge-ftui-adapter input_translation_mouse_wheel -- --nocapture`
- `cargo test -p forge-tui mouse_ -- --nocapture`
- `cargo test -p forge-tui translate_runtime_event_maps_key_resize_mouse -- --nocapture`
- `cargo build -p forge-tui`
