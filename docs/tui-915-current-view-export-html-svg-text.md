# TUI-915 current view export (HTML/SVG/text)

Task: `forge-sqv`  
Status: delivered

## Scope

- One-key export from live TUI view.
- Export writes 3 artifacts for the current frame:
  - text (`.txt`)
  - HTML (`.html`)
  - SVG (`.svg`)

## Implementation

- New export module: `crates/forge-tui/src/view_export.rs`
  - frame -> text/html/svg renderers
  - ANSI256/RGB color conversion for HTML/SVG output
  - safe escaping for HTML/XML content
  - timestamped filename helper + file writer
- Command surface:
  - `Command::ExportCurrentView` added in `crates/forge-tui/src/app.rs`
  - key: `E` in main mode
  - command palette action: `Export Current View`
- Runtime wiring:
  - command handled in `crates/forge-tui/src/interactive_runtime.rs`
  - output directory:
    - `FORGE_TUI_EXPORT_DIR` if set
    - fallback `.forge-exports/`
  - status line confirms written files + directory

## Validation

- `cargo fmt --all`
- `cargo test -p forge-tui view_export::`
- `cargo test -p forge-tui palette_enter_executes_export_action`
- `cargo test -p forge-tui export_key_dispatches_export_command`
- `cargo test -p forge-tui export_query_resolves_export_action`
- `cargo build -p forge-tui`
