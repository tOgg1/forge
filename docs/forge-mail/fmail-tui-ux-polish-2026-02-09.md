# fmail TUI UX polish pass (2026-02-09)

Scope: readability + hierarchy tweaks in the core flow `Dashboard (D) -> Topics (t) -> Thread (r) -> Search (S)`.

## Before/after notes

- Topics view
  - Before: header line packed; filter state easy to miss; selection was mostly cursor + text color.
  - After: title + hints split into clear lines; active filter highlighted; selection uses full-row background highlight; compose hint line truncates draft to avoid blowing up the panel.
- Search view
  - Before: selection indicated mainly by cursor + foreground color (could get lost with colored agent names).
  - After: selection uses a full-row background highlight while preserving per-agent color styling.

No keybinding changes; only rendering/layout tweaks.

## Manual verification checklist

1. Launch: `go run ./cmd/fmail-tui --help` (confirm flags), then start TUI normally.
2. Help overlay: `?` opens/closes; no broken layout at narrow width (try resizing terminal).
3. Dashboard (default / `D`)
   - `Tab` cycles focus; `Enter` opens thread from hot topics/live feed.
   - `Esc` returns without leaving stale focus state.
4. Topics (`t`)
   - `j/k` moves selection; selected row highlight is obvious.
   - `/` enters filter mode; filter line visibly indicates active editing; `Esc` exits filter edit without leaving the view.
   - `d` toggles topics/DM; `Enter` opens a thread; `Esc` goes back.
   - `n` opens compose; draft line stays readable; `Enter` sends; `Esc` cancels.
5. Thread (`r`)
   - Navigation unchanged (`j/k`, `Ctrl+U/D`, `Enter`, `f`).
   - Read markers and unread dots still update (spot-check by sending a message via `n`).
6. Search (`S`)
   - Selection highlight remains visible even with many colored agent names.

