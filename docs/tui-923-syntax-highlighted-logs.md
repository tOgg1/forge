# TUI-923 syntax-highlighted logs

Task: `forge-fd9`  
Status: delivered (runs output path)

## Scope

- Apply semantic token highlighting to run output lines.
- Reuse existing `log_pipeline` tokenization; no new parser.

## Implementation

- Updated `crates/forge-tui/src/runs_tab.rs`.
- Output panel now:
  - classifies each visible line lane via `classify_line`
  - tokenizes via `highlight_spans`
  - renders per-token `StyledSpan::cell` styles
- Token -> color mapping:
  - keyword -> accent
  - string -> success
  - number -> warning
  - command -> focus
  - path -> info
  - error -> error
  - muted/punctuation -> muted
  - plain -> primary text

## Regression coverage

- Added `paneled_output_applies_syntax_colors_to_tokens` in `runs_tab` tests.
- Asserts rendered output row includes non-primary token colors.

## Validation

- `cargo test -p forge-tui --lib runs_tab::tests::paneled_output_applies_syntax_colors_to_tokens -- --nocapture`
- `cargo test -p forge-tui --lib runs_tab::tests::paneled_ -- --nocapture`
- `cargo test -p forge-tui --lib runs_tab::tests::render_ -- --nocapture`
- `cargo build -p forge-tui`
