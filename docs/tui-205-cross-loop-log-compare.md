# TUI-205 cross-loop log compare with synchronized scroll

Task: `forge-n5v`  
Status: delivered

## Scope

- Render two loop logs side-by-side in Multi Logs compare mode.
- Keep shared scroll with synchronized time/line anchors.
- Surface row-level diff hints for same/different/missing lines.
- Add compare-mode interaction coverage.

## Implementation

- New module: `crates/forge-tui/src/log_compare.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`
- Multi Logs integration: `crates/forge-tui/src/multi_logs.rs`
- App state + key handling: `crates/forge-tui/src/app.rs`

### Compare model

- `synchronized_windows(...)`:
  - left pane uses shared scroll baseline
  - right pane anchors by matching timestamp token when available
  - fallback anchor uses line-ratio mapping when timestamps do not match
- `diff_hint(...)` + `summarize_diff_hints(...)`:
  - row markers: `=` same, `!` different, `<` left-only, `>` right-only

### UI behavior

- Multi Logs compare toggle: `C`
- Shared compare scroll: `u/d` and `Ctrl+u/Ctrl+d`
- Compare header includes selected pair, page info, anchor, and scroll value.
- Compare subheader includes hint counters (same/diff/left/right).

## Regression tests

Added coverage for:

- timestamp-preferred anchor synchronization
- ratio fallback synchronization
- diff-hint classification and summary counts
- compare-mode toggle render path
- compare-mode shared scroll key interactions
- rendered row-level hint glyphs in compare pane

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
