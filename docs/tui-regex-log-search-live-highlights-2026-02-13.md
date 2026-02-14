# TUI regex log search + live highlights (forge-575)

Date: 2026-02-13
Task: forge-575

## What changed
- Added regex log-search mode in `App` (`UiMode::RegexSearch`).
- Added persistent regex search state:
  - `log_regex_query`
  - `log_regex_compiled`
  - `log_regex_error`
  - `log_regex_selected_match`
- Added query/match APIs:
  - `log_regex_query()`
  - `log_regex_error()`
  - `log_regex_match_count()`
- Added regex-mode controls:
  - `R` open regex search (Logs/Runs + Expanded Logs)
  - type to edit query
  - `Backspace`, `Ctrl+U` clear
  - `j/k` (and `Ctrl+N/Ctrl+P`) jump between matches
  - `Enter` apply and return
- Added live log-pane highlighting:
  - selected regex match line prefixed with `>` and accent role
  - other regex matches prefixed with `*` and success role
  - log info row includes regex context when query active
- Search state persists across tab switches.

## Regression coverage
- `regex_search_mode_opens_on_logs_tab_and_persists_query`
- `regex_search_jumps_matches_and_highlights_selected_line`
- `regex_search_invalid_pattern_surfaces_error`

## Validation
- `cargo test -p forge-tui regex_search`
- `cargo build -p forge-tui`
