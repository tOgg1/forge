# Forge TUI Visual Polish Pass

> Superseded (2026-02-13): full rewrite direction selected.
> Use `docs/tui-frankentui-full-rewrite-plan-2026-02-13.md` and
> `docs/tui-frankentui-rewrite-workstream-plan-2026-02-13.md`.

## Context

The TUI now has bordered panels and RGB colors, but it still feels barebones and cluttered. The problems are not infrastructure — the rendering primitives (`draw_panel`, `draw_styled_text`, `fill_bg`, `draw_gauge`, `CellStyle` with bold/dim/underline) are all there. The problems are **design decisions in how those primitives are used**:

- Header dumps config metadata (`theme:default density:comfortable focus:standard`) permanently
- Tab bar uses dated `[bracket]` syntax instead of modern style-based active indicators
- Footer is a monochrome wall of text with no key vs. description distinction
- Panels are static-sized — a 3-item list takes 31 rows of empty panel
- Data is formatted as `key=value` debug output (`u:1 a:1`, `total=3 success=1`)
- No row selection highlighting — only a tiny `▸` marker
- No empty-state guidance — just blank space
- Run timeline uses tree-drawing chars (`|-`) for a flat list

**Target:** Every tab should look intentional and polished, like k9s or lazygit — not like debug log output with boxes drawn around it.

---

## 1. Clean Header Bar

**File:** `crates/forge-tui/src/app.rs` — `render_header_text()`

Before: ` Forge Loops  [Overview]  6/6 loops  theme:default  density:comfortable  focus:standard`
After:  ` Forge Loops  ●6 loops  2 running  2 err`

- Remove `[tab name]` — the tab bar already shows this
- Remove `theme:`, `density:`, `focus:` — config state shown only temporarily via status line
- Add colored fleet micro-status: `●N running` (success color), `✖N err` (error color) when on non-Inbox tabs
- For Inbox tab, keep `N threads, N unread` but drop config metadata

---

## 2. Modern Tab Bar

**File:** `crates/forge-tui/src/app.rs` — `render_styled_tab_bar()`

Before: `[1:Overview]   2:Logs    3:Runs   [4:Multi Logs]   5:Inbox`
After:  ` Overview   Logs   Runs   Multi Logs   Inbox ` with active = bold+underline+accent, inactive = dim+muted

- Drop `N:` number prefixes (keys 1-5 still work, discoverable in footer)
- Active tab: accent color, bold, underline via new `draw_underline_text()` helper
- Inactive tabs: dim, muted color
- Remove `[brackets]` — state conveyed purely by styling
- Add badge on Inbox tab when unread > 0: `Inbox ●` (small dot in accent color)

---

## 3. Polished Footer with Key Highlighting

**File:** `crates/forge-tui/src/app.rs` — footer section in `render()`

Before: `? help  q quit  ctrl+p palette  ctrl+f search  / filter  1-5 tabs  j/k sel  t/T theme  M density  Z focus  F follow`
After:  `? Help  q Quit  ^P Palette  ^F Search  / Filter  j/k Navigate` with keys in bold accent, descriptions in dim muted

- Each hint rendered as: **key** (accent, bold) + description (text_muted, dim)
- Show only top ~8 context-sensitive hints per tab
- Runs tab: `,/. Select  Enter Logs  x Layer`
- Inbox tab: `Enter Read  a Ack  h Handoff  r Reply`
- Overview: `j/k Select  2 Logs  3 Runs  4 Multi`
- Graceful truncation: drop hints from right edge when terminal is narrow

---

## 4. Overview Tab — Grouped Fields + Better Formatting

**File:** `crates/forge-tui/src/overview_tab.rs`

### 4a. Two-Column Field Layout with Groups

Before:
```
│ID: l02
│Status: RUNNING
│Runs: 22
│Dir: /repos/cluster-2
│Pool: night-shift
│Profile: prod-sre
│Harness/Auth: codex / ssh
│Last Run: 2026-02-13T12:12:00Z
│Queue Depth: 4
│Interval: 1m0s
│Max Runtime: 2h0m0s
│Max Iterations: 500
```

After:
```
│ ID: l02                  Status: ● RUNNING
│ Runs: 22                 Queue: 4
│ Dir: /repos/cluster-2
│
│ Pool: night-shift         Profile: prod-sre
│ Harness: codex            Auth: ssh
│
│ Interval: 1m0s            Max Runtime: 2h0m0s
│ Max Iterations: 500       Last Run: 2026-02-13T12:12:00Z
```

- Pair related fields side-by-side in two columns (left half, right half of panel)
- Blank lines between groups (identity, location, config, timing)
- Status gets colored dot prefix: `● RUNNING` (green), `✖ ERROR` (red), `■ STOPPED` (yellow)
- Reduces vertical footprint from 12 rows to ~9 rows, freeing space for Run Snapshot

### 4b. Fleet Hero — Add Labels to Icons
Before: `●2    ○2    ■0    ✖2    │ q:30│ 33% ok`
After:  `●2 run  ○2 sleep  ■0 stop  ✖2 err  │ q:30  │ 33% ok`

### 4c. Run Snapshot — Human-Readable Format
Before: `total=3  success=1  error=1  killed=1  running=0`
After:  `3 runs  ●1 ok  ✖1 err  ■1 killed` with colored status icons

---

## 5. Runs Tab — Shrink-to-Fit + Table Columns + Row Highlight

**File:** `crates/forge-tui/src/runs_tab.rs` — `render_runs_paneled()`

### 5a. Shrink Timeline Panel to Content
- Timeline height = `min(run_count + 2, max_available * 0.6)` — panel shrinks with few items
- Output panel gets remaining vertical space
- Eliminates the 28-rows-of-blank-space problem

### 5b. Column-Aligned Table Layout

Before: `▸|- run-0172 [ERR ] [exit:1] [4m12s] prod-sre`
After:
```
│   ID        Status  Exit  Duration  Profile
│ ▸ run-0172  ERR       1   4m12s     prod-sre
│   run-0171  OK        0   3m09s     prod-sre
│   run-0170  STOP    137   45s       prod-sre
```

- Remove tree chars (`|-`, `` `- ``) — runs are a flat list, not a tree
- Remove `[brackets]` around every field — use column alignment + color
- Add column header row with dim text
- Status column: colored (`pal.success` for OK, `pal.error` for ERR, `pal.text_muted` for STOP)

### 5c. Full-Width Row Selection Highlight
- Selected row gets `pal.panel_alt` background across entire row width (via `fill_bg` before drawing text)
- `▸` marker remains as additional visual cue
- Unselected rows keep `pal.panel` background

---

## 6. Inbox Tab — Subject-First Thread List + Cleaner Stats

**File:** `crates/forge-tui/src/app.rs` — `render_inbox_pane()`

### 6a. Thread List — Subject First, Badges Right

Before: `▸ m-33 u:1 a:1 incident forge-333`
After:  `▸ incident forge-333          ●1 new  ⚑1 ack`

- Lead with subject (the meaningful text), push badges to right-aligned position
- Replace `u:1` with `●N new` in accent/info color (unread indicator)
- Replace `a:1` with `⚑N ack` in warning color (pending acknowledgment)
- Drop mail ID from list view (show in detail panel header)

### 6b. Filter Stats Bar — Colored Badges

Before: `Inbox filter:all  threads:2  unread:2  pending-ack:2  claims:3  conflicts:1`
After:  `Inbox  all ▾  2 threads  ●2 unread  ⚑2 pending  ⚡1 conflict`

- Use colored text for each badge: unread (info), pending (warning), conflict (error)
- Drop `filter:` and `claims:` labels — implicit from context

### 6c. Move Keybinding Hints Out of Detail Panel
- The detail panel currently shows `enter=read  a=ack  h=handoff...` as its first content line
- Move these to the footer (tab-specific hints from section 3)
- First line of detail panel becomes the first message preview instead

---

## 7. Multi-Logs — Collapse Meta + Empty State

**File:** `crates/forge-tui/src/multi_logs.rs`

### 7a. Single Meta Line
Before (2 lines):
```
View 4 Matrix  requested=2x2 effective=2x2  page=1/2  showing=1-4/6
layer:raw  pin:<space> clear:c  compare:C  layout:m  page:,/. g/G  order:pinned first
```
After (1 line):
```
2x2 grid  page 1/2  (1-4 of 6)  layer:raw
```

- Remove `View 4 Matrix`, `requested=`, `effective=`, `order:` debug metadata
- Move keybinding hints to footer
- Saves 1 row of vertical space per grid

### 7b. Colored Health Status
Before: `ERROR    q=0 runs=20 health=err harness=codex`
After:  `● ERROR  q:0  runs:20  codex` with status word in `pal.error` color

- Status word colored semantically (red ERROR, green RUNNING, dim WAITING)
- Remove redundant `health=err` (same info as status)

### 7c. Empty Pane Content
When log pane has no output lines, show `Waiting for output...` in dim text instead of blank space.

---

## 8. Adapter Helpers

**File:** `crates/forge-ftui-adapter/src/lib.rs`

Add two convenience methods to `RenderFrame`:

```rust
pub fn draw_dim_text(&mut self, x: usize, y: usize, text: &str, fg: TermColor, bg: TermColor)
// Same as draw_styled_text but sets dim: true — for footer descriptions, empty states

pub fn draw_underline_text(&mut self, x: usize, y: usize, text: &str, fg: TermColor, bg: TermColor, bold: bool)
// Same as draw_styled_text but sets underline: true — for active tab indicator
```

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/forge-ftui-adapter/src/lib.rs` | Add `draw_dim_text()`, `draw_underline_text()` |
| `crates/forge-tui/src/app.rs` | Header cleanup, tab bar modernization, footer key highlighting, inbox thread list, filter bar |
| `crates/forge-tui/src/overview_tab.rs` | Two-column grouped fields, fleet hero labels, run snapshot formatting |
| `crates/forge-tui/src/runs_tab.rs` | Shrink-to-fit panels, table column alignment, row selection highlight, header row |
| `crates/forge-tui/src/multi_logs.rs` | Collapse meta lines, colored health status, empty-state text |
| `crates/forge-tui/tests/golden/layout/*.txt` | All 15 golden snapshots regenerated |

## Verification

1. `cargo build -p forge-ftui-adapter -p forge-tui` — clean compile, no warnings
2. `cargo test -p forge-ftui-adapter -p forge-tui` — update assertions in tests that check specific text content
3. `UPDATE_GOLDENS=1 cargo test -p forge-tui --test layout_snapshot_test` — regenerate snapshots
4. Read each golden file at 120x40 and verify: no debug-like text, no wasted space, clear visual hierarchy
5. Verify 80x24 breakpoint: content doesn't truncate badly, panels shrink gracefully
