# TUI-931: Terminal Picture-in-Picture Mini-Views

Task: `forge-pey`

## Scope
- Pin any panel payload as a floating terminal mini-view.
- Support multiple PiP windows, corner anchoring, and focus cycling.
- Add in-terminal controls for opacity, size, collapse, and move offsets.

## Implementation
- Added `crates/forge-tui/src/picture_in_picture.rs`.
- Core state model:
  - `PiPSource`, `PiPAnchor`, `PiPWindow`, `PiPState`, `PiPRenderWindow`.
- Core behaviors:
  - `pin_pip_window` (dedupe by source, auto-focus, max-window eviction)
  - `unpin_pip_window`
  - `focus_next_pip_window`
  - `set_pip_opacity` (clamped 20-100)
  - `resize_pip_window` (clamped width/height)
  - `move_pip_window_to_anchor`
  - `toggle_pip_collapsed`
  - `render_pip_windows` (corner placement + stacking + compact collapsed render)

## Regression Tests
- `picture_in_picture::tests::pin_same_source_updates_existing_window`
- `picture_in_picture::tests::pin_enforces_max_windows_by_dropping_oldest`
- `picture_in_picture::tests::opacity_and_resize_clamp_bounds`
- `picture_in_picture::tests::render_places_windows_in_each_corner`
- `picture_in_picture::tests::collapsed_window_renders_compact_lines_and_focus_cycle`

## Validation
- `cargo fmt --package forge-tui`
- `cargo test -p forge-tui picture_in_picture::tests:: -- --nocapture`
- `cargo check -p forge-tui`
- `cargo build -p forge-tui`
