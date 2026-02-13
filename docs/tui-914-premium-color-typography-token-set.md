# TUI-914 premium color + typography token set (ANSI16/256/truecolor)

Task: `forge-pqq`  
Status: delivered

## Scope

- Upgraded adapter theme token vocabulary to include `warning`, `info`, and `focus` slots.
- Added typography policy tokens at theme level:
  - bold emphasis for accent/severity roles
  - dimmed metadata for richer terminals
  - underlined focus treatment for keyboard visibility
- Wired typography tokens through render cell style (`bold`, `dim`, `underline`) and terminal output.
- Updated TUI status line to render informational states via `info` token role.

## ANSI capability behavior

- ANSI16: high-contrast-safe emphasis; focus underline preserved.
- ANSI256/truecolor capability path: richer accent/info/warning separation, dim metadata enabled.
- Runtime still emits ANSI palette indexes; token mapping now explicitly models capability-aware hierarchy.

## Regression coverage

- `crates/forge-ftui-adapter/src/lib.rs`
  - token snapshot now includes warning/info/focus
  - focus role asserts underline semantics
  - muted role asserts dim semantics (dark theme)
- `crates/forge-tui/tests/layout_snapshot_test.rs`
  - refreshed overview goldens for current command-center layout behavior

## Validation

- `cargo fmt --check`
- `cargo clippy -p forge-tui --all-targets -- -D warnings`
- `cargo test -p forge-tui`
- `cargo test -p forge-ftui-adapter`
