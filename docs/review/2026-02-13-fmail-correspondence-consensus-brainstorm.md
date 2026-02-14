# TUI Correspondence Review + Feature Brainstorm Consensus (2026-02-13)

## Scope reviewed
- `fmail` topic: `tui-visual-polish`
- Related DMs to WS owners/reviewers: `@lunar-krabappel`, `@gentle-mccoy`, `@tui-polish-lead`, `@trmd-opus`, `@spry-todd`
- Time window: 2026-02-13 18:16 to 19:53 local thread timestamps

## Consensus state
- Decision lock present in thread: **direct full rewrite on true FrankenTUI**.
- Superseded: incremental polish / bridge-first path.
- No `BLOCKED` responses seen in topic for rewrite direction.

## Locked guardrails
1. WI-0 safety first: Unicode-safe truncation + narrow-width guards.
2. Parity gates before default flip: snapshots/tests green at `80x24`, `120x40`, `200x50`.
3. No implicit interactive fallback; dev-only explicit fallback allowed.
4. Per-work-item commits (conventional style).
5. Preserve state/keymap/theme/data logic; replace runtime/rendering path.

## Owner/ETA map (thread consensus)
- WS-0 Safety foundation: `lunar-krabappel` (ETA Feb 13 23:00)
- WS-1 Runtime root: `gentle-mccoy` (ETA Feb 14 10:00)
- WS-2 Adapter upstream bridge: `tui-polish-lead` (ETA Feb 14 18:00) — accepted
- WS-3 Shell rewrite: `trmd-opus` (ETA Feb 15 12:00)
- WS-4 Overview/Runs: `tui-polish-lead` (ETA Feb 15 20:00) — accepted
- WS-4 Logs/MultiLogs/Inbox: `gentle-mccoy` (ETA Feb 16 12:00)
- WS-5 Gates/cutover/sunset: `lunar-krabappel` (ETA Feb 16 18:00)

## Brainstorm synthesis

### P0 (week 1/2) consensus shortlist
1. Runs triage table + sticky output (`Table` + `Panel` + `Flex`)
   - Metric: failing-run output reachable <=10s keyboard-only.
2. Incident-first inbox queue (`List` + `Badge` + `Panel`)
   - Metric: highest-priority unread thread <=3 keypresses.
3. Multi-loop health wall (`Flex/Grid` + `Panel` + `LogViewer`)
   - Metric: degraded loop among 6 detectable <=5s scan.
4. Command surface bar (`StatusLine` + `KeyHint`)
   - Metric: first-time tab/filter/palette flow <60s.
5. Modal action rail (`ModalStack` + `NotificationQueue`)
   - Metric: zero accidental destructive confirms in smoke run.

### P1 (post-cutover)
- Markdown inbox detail
- Syntax-highlight logs
- Clipboard copy actions
- Run-output diff view
- Export snapshots
- Failure Explain Strip (promote to P0 only if scope slack)

## Architecture seams requested by reviewers
- Keep runtime `Model` ownership in WS-1 (avoid WS-2 scope bleed).
- Delay default-enabling upstream feature until WS-1 smoke passes.
- Add WS-2 event parity tests (keyboard/resize/focus), not only cell-style parity.
- Preserve extension seams now:
  - Log source abstraction (parsed logs vs PTY vs diff)
  - Styled-span panel inputs (markdown/syntax integration)
  - App-wide notification bus (toasts/failure explain/clipboard feedback)

## Open confirmations
- Thread has strong ACK signal; explicit fresh ACK/BLOCKED replies to latest synthesis still pending from some WS owners.
- If no objections by requested cutoff (20:00 local in-thread), treat P0/P1 slate as consensus backlog.

## Addendum (latest thread pass)
- Additional correspondence reviewed through timestamp `20260213-191722`.
- No `BLOCKED` responses surfaced; latest reviewers endorse locked P0 unchanged for scope control.

### Net-new feature ranking from latest brainstorm
Low-risk/high-impact (`P1-fast` candidates):
1. Cross-tab deep links (Overview -> Runs -> Logs context jumps)
   - Metric: error seen in Overview to failing-run logs <=3 keypresses.
2. Evidence hotkeys (jump to latest ERROR/WARN/ACK with return point)
   - Metric: latest relevant event reachable <=2 keypresses.
3. Breadcrumb backtrack in statusline (one-key return from jump source)
   - Metric: return to source context <=1 keypress in 95% cases.
4. Loop lifecycle timeline strip (recent state transition visualization)
   - Metric: identify last error-start time <=2s.

Bold/high-risk prototype backlog:
1. Incident playback strip (time scrub synced across Overview/Runs/Logs)
   - Metric: reconstruct root-cause sequence <=30s.
2. Semantic incident map overlay (loop/run/inbox graph links)
   - Metric: locate root failing run + related thread <30s for first-time operator drill.

### Architecture correspondence update
- Latest consensus recommendation: **Option A** (`forge-tui` implements `Model` directly).
- Adapter role narrowed to Forge widget/style kit + translators; avoid runtime mediation wrapper indirection.

## Addendum 2 (lock-round responses)
- Additional correspondence reviewed through timestamp `20260213-191904`.
- New explicit lock-round responses received from `trmd-opus` and `tui-polish-lead`.

### Lock-round decisions (current)
1. Runtime architecture:
   - Converged recommendation remains **Option A** (direct `Model` ownership in `forge-tui`).
   - Adapter scope reduced to style/theme translation + Forge widget factories.
2. Nightly blocker:
   - `ftui` widget-only dependency path (`default-features = false`) confirmed compiling on stable by `tui-polish-lead`.
   - Full rewrite runtime path (`runtime`, `crossterm`) still **unverified on stable**; WS-1 must run compile check immediately.
3. P0 scope control:
   - Multiple reviewers reaffirm: keep P0 unchanged.

### Emerging post-P0 priorities
P1-fast candidates:
1. Next-Action panel (Overview rules-based "what to do now")
   - Metric: new operator identifies correct first action within <=5s.
2. Cross-tab deep links + breadcrumb return
   - Metric: Overview error -> failing run output <=3 keypresses; return <=1 keypress.
3. Clickable link registration in logs/runs/inbox contexts
   - Metric: supported terminals open linked artifact in one interaction.

P2/P3 notes:
- P2: claim timeline rail, UI-state undo/redo, state persistence, evidence hotkeys.
- P3/prototype: incident playback strip, semantic incident map overlay.

## Addendum (latest thread pass 2)
- Additional correspondence reviewed through timestamp `20260213-191741`.
- New proposals from `tui-polish-lead` and follow-up reviewer notes do not conflict with locked P0/P1; they mostly expand post-cutover opportunity space.

### Additional feature candidates (new since previous cutoff)
Low-risk/high-impact (recommended `P1-fast` / `P2` depending capacity):
1. Cross-tab deep links and breadcrumb backtrack
   - Why: removes tab-silo friction during triage.
   - Metric: Overview error -> failing run logs in <=3 keypresses; jump-back in <=1 keypress (95%).
2. Evidence hotkeys (jump to latest ERROR/WARN/ACK with sticky return point)
   - Why: faster event localization without manual scrolling.
   - Metric: latest relevant event reachable in <=2 keypresses.
3. Loop lifecycle timeline strip
   - Why: quickly answers "when did degradation start?"
   - Metric: operator identifies last error-start time in <=2s.
4. Error boundaries per panel
   - Why: isolate pane failures; avoid full-TUI collapse.
   - Metric: pane fault shows local error state while other panes remain interactive.
5. Responsive breakpoint contracts
   - Why: deterministic narrow-terminal degradation behavior.
   - Metric: no layout overlap/clipping across declared tiers at 80x24/120x40/200x50.

Bold/high-risk (prototype backlog):
1. Incident playback strip (time-scrub synchronized across tabs)
   - Metric: reconstruct root-cause sequence in <=30s.
2. Semantic incident map overlay (loop/run/inbox graph)
   - Metric: locate failing run + related thread in <30s for first-time operator.

### Additional architecture notes from latest correspondence
- Recommended runtime boundary remains: `forge-tui` owns direct `Model` implementation (Option A).
- Adapter remains valuable for:
  - Forge-specific widget factories/style translation.
  - Bridge helpers and parity tests.
- New "possible soon" candidates enabled by full FrankenTUI runtime:
  - Undo/redo support for table/list selection/filter actions.
  - State persistence across restarts (tab/selection/scroll restore).
  - Degradation budget support for low-fidelity rendering on constrained terminals.
  - Link registry integration (OSC8-compatible clickable links).
  - Hit-grid-driven mouse interactions.
- Recommended handling: keep these out of locked P0; treat as post-cutover `P2` queue unless explicit owner capacity appears after parity gates.

## Addendum (latest thread pass 3)
- Additional correspondence reviewed through timestamp `20260213-192020`.
- Lock-round prompts and responses captured from multiple agents (`trmd-opus`, `tui-polish-lead`, `lively-stotch`, `lunar-jacqueline`, `spry-todd`).

### Blocker status updates
1. Nightly requirement blocker:
   - Initially raised as critical from upstream getting-started docs.
   - Latest verification in-thread now confirms stable compatibility for pinned rev `23429fac0e739635c7b8e0b995bde09401ff6ea0` with:
     - `features=[runtime]`
     - `features=[runtime,crossterm]`
   - Outcome: nightly blocker is currently **denied** for tested combinations at pinned rev.
2. Runtime architecture:
   - Convergence remains on **Option A** (direct `Model` ownership in `forge-tui`).
   - Adapter remains focused on Forge-specific widget/style translation, parity helpers, and bridge surface.

### Feature prioritization updates from latest pass
- Strong additional support for:
  - `Next-Action Panel` (Overview rules-based suggestions, actionable jumps)
  - `Claim Timeline` rail (Inbox conflict/claim sequence visibility)
- Recommended placement:
  - `Next-Action Panel`: high-priority post-cutover (`P1-fast`)
  - `Claim Timeline`: post-cutover (`P2`) unless capacity appears

### Latest practical priority framing
- P0 remains unchanged (scope lock preserved).
- P1-fast likely queue now includes:
  - Next-Action Panel
  - Cross-tab deep links
  - Evidence hotkeys
  - Breadcrumb backtrack
  - Clickable links
- Bold/high-risk prototypes remain deferred.

## Addendum (latest thread pass 3)
- Additional correspondence reviewed through timestamp `20260213-192013`.
- New signals include:
  - `lively-stotch` pass-3 synthesis doc added (`docs/review/2026-02-13-fmail-correspondence-brainstorm-pass3.md`).
  - `spry-todd` lock-round reply explicitly **denies nightly blocker for tested combos** and reports stable compile success for runtime paths tested at pinned rev.
  - No fresh reply yet from `@gentle-mccoy` (WS-1 owner) or `@lunar-krabappel` (WS-5 owner) despite repeated direct requests.

### Updated blocker status (provisional)
1. Nightly requirement:
   - Earlier state: runtime path unverified on stable.
   - Latest state: reviewer report (`spry-todd`) indicates stable passes for tested runtime combos at pin.
   - Remaining gap: WS-1 owner has not yet posted explicit confirmation in-thread.
2. Runtime architecture:
   - Still converged on Option A (direct `Model` in `forge-tui`).

### Feature consensus delta
- No change to locked P0.
- Reinforced post-P0 priority:
  - P1-fast: Next-Action Panel, Cross-Tab Deep Links + Breadcrumb Return, Clickable Links.
  - P2: Claim Timeline rail, Undo/Redo (UI state), State Persistence, Evidence Hotkeys.
  - P3: Incident Playback Strip, Semantic Incident Map.

### Open follow-up to close
- Request explicit WS-1 and WS-5 confirmations on:
  1. stable compile stance for `runtime`/`crossterm`,
  2. any objections to P1-fast ordering.
