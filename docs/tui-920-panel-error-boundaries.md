# TUI-920 panel error boundaries

Task: `forge-xv8`  
Status: delivered

## Scope

- Prevent a single pane panic from crashing the full TUI.
- Render a local fallback panel with cause detail.

## Implementation

- New module: `crates/forge-tui/src/panel_error_boundary.rs`
- API: `render_panel_with_boundary(panel_name, size, theme, palette, render)`
- Behavior:
  - catches pane panic payloads (`String`/`&'static str`)
  - paints contextual fallback message in-pane
  - preserves app-level runtime continuity

## Validation

- `cargo test -p forge-tui --lib panel_error_boundary::tests::`
- `cargo build -p forge-tui`
