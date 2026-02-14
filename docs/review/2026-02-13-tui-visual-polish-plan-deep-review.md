# Deep Review: `docs/tui-visual-polish-plan.md`

Date: 2026-02-13
Reviewer: codex (`lunar-krabappel`)

## Scope reviewed

- Plan doc: `docs/tui-visual-polish-plan.md`
- Program constraints/docs:
  - `docs/tui-907-visual-parity-checklist-target-screenshots.md`
  - `docs/tui-909-layout-snapshot-breakpoint-gate.md`
  - `docs/tui-601-fmail-inbox-panel.md`
  - `docs/tui-913-runs-pane-timeline-badges-duration-chips.md`
- Implementation:
  - `crates/forge-tui/src/app.rs`
  - `crates/forge-tui/src/overview_tab.rs`
  - `crates/forge-tui/src/runs_tab.rs`
  - `crates/forge-tui/src/multi_logs.rs`
  - `crates/forge-ftui-adapter/src/lib.rs`

## Baseline verification

- `cargo test -p forge-tui --test layout_snapshot_test`: pass
- `cargo test -p forge-tui --lib`: pass (634 passed, 0 failed, 1 ignored)

## Findings (ordered by severity)

### 1. Critical: new Unicode labels/icons can panic current truncation code

Evidence:
- Plan introduces non-ASCII glyphs in header/footer/inbox/runs (`●`, `✖`, `⚑`) in `docs/tui-visual-polish-plan.md:25`, `docs/tui-visual-polish-plan.md:45`, `docs/tui-visual-polish-plan.md:54`, `docs/tui-visual-polish-plan.md:154`.
- App still slices strings by byte index in multiple places, e.g. `crates/forge-tui/src/app.rs:3295`, `crates/forge-tui/src/app.rs:3312`, `crates/forge-tui/src/app.rs:3369`, `crates/forge-tui/src/app.rs:3511`.

Impact:
- Runtime panic on narrow widths when truncation boundary cuts through multibyte char.

Required fix before polish pass:
- Replace all `[..width]` byte slicing with one width-safe utility (char-safe minimum; display-width-safe preferred).

---

### 2. High: plan removes critical header state currently required by behavior/tests/docs

Evidence:
- Plan removes `theme/density/focus` from header (`docs/tui-visual-polish-plan.md:27`, `docs/tui-visual-polish-plan.md:28`).
- Current header intentionally surfaces mode/focus/follow state in `crates/forge-tui/src/app.rs:3359`.
- Tests assert these states in header:
  - deep focus signal: `crates/forge-tui/src/app.rs:4774`
  - follow ON/OFF signal: `crates/forge-tui/src/app.rs:4937`, `crates/forge-tui/src/app.rs:4954`
- Parity checklist still expects informative header with theme/mode/focus in `docs/tui-907-visual-parity-checklist-target-screenshots.md:48`.

Impact:
- Operator loses visible mode/focus/follow context.
- Existing parity gate/test contract broken.

Recommendation:
- Keep critical state chips, but compress visual form:
  - keep `focus` + `follow`
  - move theme/density to compact chips only when non-default or recently changed.

---

### 3. High: plan conflicts with Inbox parity contract by dropping message IDs

Evidence:
- Plan says drop mail ID in list (`docs/tui-visual-polish-plan.md:159`).
- Inbox parity doc requires CLI parity ID display (`docs/tui-601-fmail-inbox-panel.md:10`).
- Test explicitly checks ID visibility: `crates/forge-tui/src/app.rs:4376`.

Impact:
- Harder to cross-reference with `forge mail`/logs and handoffs.
- Regresses documented parity.

Recommendation:
- Subject-first layout yes; keep compact ID suffix/prefix (dim) for parity.

---

### 4. High: target says “every tab polished”, but Logs tab is still placeholder and excluded

Evidence:
- Plan target: “Every tab should look intentional” in `docs/tui-visual-polish-plan.md:16`.
- Plan sections cover header/tab/footer + Overview/Runs/Inbox/Multi-Logs only.
- Logs tab currently falls back to placeholder text path in `crates/forge-tui/src/app.rs:3266` (visible in golden `crates/forge-tui/tests/golden/layout/logs_80x24.txt:3`).

Impact:
- Largest perceived quality gap remains after pass.

Recommendation:
- Add explicit section for Logs tab polish in this same plan (at least minimal real pane + empty/loading/error states).

---

### 5. Medium: proposed Runs “shrink-to-fit” formula can starve output pane on short terminals

Evidence:
- Proposed formula in `docs/tui-visual-polish-plan.md:120` has no hard minimum output rows.
- Current code protects output area via `min_output_rows` in `crates/forge-tui/src/runs_tab.rs:322`.

Impact:
- 80x24 and smaller sizes can lose readable output context.

Recommendation:
- Keep hard minimum output rows + bounded timeline fraction.

---

### 6. Medium: tab discoverability risk if number prefixes removed without guaranteed fallback

Evidence:
- Plan removes `1-5` prefixes (`docs/tui-visual-polish-plan.md:41`).
- Plan also proposes fewer hints and right-edge truncation (`docs/tui-visual-polish-plan.md:57`, `docs/tui-visual-polish-plan.md:61`).
- Numeric key navigation is core and documented in keymap (`crates/forge-tui/src/keymap.rs:239` onward).

Impact:
- New users may lose obvious path to tab switching.

Recommendation:
- Keep subtle numeric affordance (e.g. dim superscript or optional compact hint segment) until command palette/onboarding coverage proves enough.

---

### 7. Medium: adapter API proposal is too narrow and duplicates style semantics

Evidence:
- Plan proposes `draw_dim_text`/`draw_underline_text` only (`docs/tui-visual-polish-plan.md:211`).
- Adapter already has role-driven dim/underline via `style_for_role` in `crates/forge-ftui-adapter/src/lib.rs:892`.

Impact:
- API bloat; inconsistent style pathways.

Recommendation:
- Prefer one generalized styled-text API (`bold/dim/underline`) or role + explicit bg API.

---

### 8. Medium: verification checklist missing key quality gates

Evidence:
- Plan verification only covers build/tests/goldens (`docs/tui-visual-polish-plan.md:236` onward).
- Repo policy expects lint/type/build and existing perf/accessibility/reliability checks.

Impact:
- Possible regressions in perf/contrast/capability paths despite passing snapshots.

Recommendation:
- Add:
  - `cargo fmt --check`
  - `cargo clippy -p forge-tui --all-targets -- -D warnings`
  - `cargo test -p forge-tui`
  - targeted ANSI16/ANSI256/truecolor checks
  - performance gate tests (`performance_gates`)

## Suggested improved plan shape (v2)

1. Safety foundation first
- Add width-safe truncation utility; remove all byte slicing in `app.rs`.
- Add tests for Unicode truncation safety in header/footer/tab/inbox lines.

2. Shell polish (no state loss)
- Header: compact chips, but retain critical `focus`/`follow`; conditional theme/density.
- Tab bar: modern style; keep lightweight key discoverability.
- Footer: context-aware hint model with explicit priority order and guaranteed core keys.

3. Pane polish
- Overview: responsive two-column only when width threshold met, else single-column fallback.
- Runs: column layout + selected-row background; preserve min output rows.
- Inbox: subject-first + badges, keep compact message ID token.
- Multi-logs: compact meta line in standard mode; preserve compare-mode diagnostics.
- Logs: add real content lane so “every tab polished” is true.

4. Verification
- Full lint/test gates.
- Regenerate and review 15 layout goldens.
- Add/refresh targeted text-assert tests (not just goldens) for header state chips, inbox ID parity, and compare-mode metadata.

## Net assessment

The visual direction is strong and mostly right. Main blockers are safety (Unicode truncation panic risk), parity contract breaks (header state + inbox IDs), and missing Logs scope.

With those corrected, this can land as a high-confidence polish pass.

## External benchmark notes (FrankenTUI)

Reference links provided by user:
- https://frankentui.com/web?zoom=0.46
- https://github.com/Dicklesworthstone/frankentui

High-value patterns to port:

1. Width-safe display math as a first-class invariant
- FrankenTUI chrome uses explicit display-width helpers (`ftui_text::display_width`) and repeatedly documents deterministic width/capability behavior.
- Port: replace byte slicing with a shared Forge helper that is Unicode-safe and width-aware. Gate with tests at `80x24`, `120x40`, `200x50`.

2. Explicit degradation paths for small terminals
- Their HUD spec has `full -> compact -> minimal` fallback states with deterministic rules.
- Port: define fallback contracts for each pane (esp. Runs + Inbox) so narrow terminals degrade by feature, not by broken truncation.

3. Deterministic evidence-first UX gates
- Their specs consistently define deterministic output + explicit failure modes and test matrices.
- Port: extend `docs/tui-visual-polish-plan.md` with explicit failure-mode checks: no panics on Unicode truncation, no hidden key discoverability regressions, no compare-mode metadata loss.

4. Keyboard policy as a formal contract
- FrankenTUI keybinding policy is state-machine based and deterministic with tie-break/timeout rules.
- Port: keep tab-switch discoverability as a contract (header/tab/footer/onboarding combined), not a styling preference.

5. Responsive layout contracts, not ad-hoc heuristics
- Their responsive demo and resize scheduler specs formalize breakpoint behavior and coalescing strategy.
- Port: add concrete breakpoints for overview two-column layout and runs timeline/output split, with explicit fallback thresholds.

Net: their strongest contribution is not just visual style; it is explicit behavioral contracts around rendering, resize, and input. Applying that mindset will reduce regressions during this polish pass.
