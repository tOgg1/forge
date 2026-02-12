# TUI-102 Command Palette Core (forge-8dc)

Date: 2026-02-12
Scope: `Ctrl+P` palette, typed action registry, deterministic fuzzy ranking, recency bias, latency budget.

## Core behavior

- Entry: `Ctrl+P` from main mode.
- Exit: `Esc` or `q`.
- Query edit: type chars, `Backspace` or `Ctrl+H`.
- Selection: `Tab`, `j/k`, `Up/Down`.
- Execute: `Enter`.

## Registry model

Typed action ids (`PaletteActionId`) with default registry entries:

- tab switches: overview/logs/runs/multi-logs
- filter open
- new loop wizard
- selected-loop actions: resume/stop/kill/delete
- theme cycle
- zen toggle

Each action stores:

- id
- title
- command phrase
- keyword list
- preferred tab (optional)
- requires selection (bool)

## Ranking

Final score = query match + context bonus + recency bonus.

- Query match:
  - exact > prefix > substring > ordered-subsequence fuzzy.
- Context bonus:
  - preferred tab matches current tab.
  - selected-loop actions only eligible when loop selected.
- Recency bias:
  - last-used sequence + usage count bonus.

Tie-breaks are deterministic:

1. score (descending)
2. title (ascending)
3. command (ascending)

## Latency budget

- Search loop enforces explicit budget (`DEFAULT_SEARCH_BUDGET = 4ms`).
- Result includes `timed_out` marker if budget exceeded.

## Source of truth

- Core: `crates/forge-tui/src/command_palette.rs`
- App integration: `crates/forge-tui/src/app.rs`
