# Inbox Detail Markdown Rendering (2026-02-13)

Task: `forge-qbh`

## What changed

- Inbox detail pane now renders markdown body content for the selected thread's latest message under a dedicated section.
- Thread message rows now include richer per-message detail:
- Subject line (when present).
- Markdown-aware body lines with wrapping.
- Added markdown line formatter helpers in `crates/forge-tui/src/app.rs`:
- Headings (`#`, `##`, `###`) preserved with heading prefixes.
- Bulleted and numbered lists preserved.
- Quotes (`>`) preserved.
- Code fences rendered as code-prefixed lines.
- Wrapped continuation indentation for list/quote/code/plain text.

## Regression coverage

- `inbox_detail_pane_renders_markdown_body`
- `inbox_markdown_detail_lines_preserve_markdown_structure`
- `inbox_markdown_detail_lines_wrap_long_words_and_continuations`

## Validation

- Passed: `cargo test -p forge-tui inbox_detail_pane_ -- --nocapture`
- Passed: `cargo test -p forge-tui inbox_markdown_detail_lines_ -- --nocapture`
- Attempted: `cargo build -p forge-tui`
- Current repo head has unrelated compile blocker in `crates/forge-tui/src/blocker_graph.rs` (`E0689` ambiguous integer type on `saturating_add`), so full build is not clean at this moment.
