# Deep Review: Forge TUI Improvement Strategy

Date: 2026-02-13
Reviewer: codex (`tactful-stewie`)

## Scope

Reviewed:
- `docs/tui-next-roadmap-2026-02-10.md`
- `docs/tui-visual-polish-plan.md`
- `docs/tui-907-visual-parity-checklist-target-screenshots.md`
- `docs/tui-909-layout-snapshot-breakpoint-gate.md`
- `docs/tui-912-frankentui-shell-single-root-bootstrap.md`
- `docs/tui-913-runs-pane-timeline-badges-duration-chips.md`
- `docs/tui-914-premium-color-typography-token-set.md`
- `docs/review/2026-02-13-tui-visual-polish-plan-deep-review.md`
- fmail threads: `tui-visual-polish`, `task`
- current render code and goldens:
  - `crates/forge-tui/src/app.rs`
  - `crates/forge-tui/src/runs_tab.rs`
  - `crates/forge-tui/src/multi_logs.rs`
  - `crates/forge-tui/tests/golden/layout/*.txt`

External benchmark reference (FrankenTUI):
- https://frankentui.com/web?zoom=0.46
- https://github.com/Dicklesworthstone/frankentui
- Local doc pass from cloned repo:
  - `docs/risk-register.md`
  - `docs/spec/keybinding-policy.md`
  - `docs/spec/mermaid-config.md`
  - `docs/telemetry.md`
  - `docs/performance-hud.md`
  - `docs/one-writer-rule.md`

## Baseline checks run

- `cargo test -p forge-tui --test layout_snapshot_test` -> PASS (1/1)

## Current state (high signal)

- Superdash implementation wave completed (non-EPIC leaf tasks closed) per fmail `task` stream (`forge-9r4`, `forge-p6h`, `forge-hsr`, `forge-r62`, `forge-0sx`, `forge-333`, `forge-6fe`, `forge-qbx`, `forge-wze`).
- Layout breakpoints are gated (`80x24`, `120x40`, `200x50`) and passing (`docs/tui-909-layout-snapshot-breakpoint-gate.md`).
- Runtime path is now single-root interactive FrankenTUI bootstrap (`docs/tui-912-frankentui-shell-single-root-bootstrap.md`).

But visual/UX strategy still inconsistent in-tree:
- `logs_80x24` golden still shows placeholder content (`crates/forge-tui/tests/golden/layout/logs_80x24.txt`).
- Header/footer/tab patterns remain debug-heavy and bracket-heavy (`crates/forge-tui/src/app.rs:3310`, `crates/forge-tui/src/app.rs:3360`, `crates/forge-tui/src/app.rs:3419`).
- Inbox remains metadata-first (`m-33 u:1 a:1 ...`) (`crates/forge-tui/src/app.rs:3624`).

## Findings (ordered by severity)

### 1) Critical: strategy still lacks one hard invariant for width-safe text handling

Evidence:
- byte slicing remains in active render paths:
  - `crates/forge-tui/src/app.rs:3168`
  - `crates/forge-tui/src/app.rs:3182`
  - `crates/forge-tui/src/app.rs:3296`
  - `crates/forge-tui/src/app.rs:3313`
  - `crates/forge-tui/src/app.rs:3370`
  - `crates/forge-tui/src/app.rs:3512`
  - `crates/forge-tui/src/app.rs:3892`
- polish RFC adds more unicode/iconography (fmail `tui-visual-polish` thread).

Impact:
- panic risk or broken truncation boundaries under narrow widths + multibyte text.
- recurring regressions as polish work expands.

External benchmark:
- FrankenTUI treats unicode width as explicit risk class + mitigation stack (`docs/risk-register.md`, R3 section).

Recommendation:
- define single text-clamp contract and ban direct `[..width]` in render paths.
- add unicode corpus tests for header/footer/inbox/runs/multi lines.

### 2) High: plan contract drift between parity docs, code tests, and polish proposal

Evidence:
- parity checklist expects informative header signals (`docs/tui-907-visual-parity-checklist-target-screenshots.md`).
- tests assert header focus/follow visibility (`crates/forge-tui/src/app.rs:4774`, `crates/forge-tui/src/app.rs:4937`, `crates/forge-tui/src/app.rs:4954`).
- Inbox parity relies on message IDs (`docs/tui-601-fmail-inbox-panel.md`, `crates/forge-tui/src/app.rs:3625`, `crates/forge-tui/src/app.rs:4376`).
- initial polish doc removes/changes these signals (`docs/tui-visual-polish-plan.md`).
- fmail thread already revised plan to keep critical state/IDs, but doc not updated yet (messages around `20260213-181953`, `20260213-182029`).

Impact:
- implementation churn, broken tests/goldens, and subjective debates.

Recommendation:
- immediately merge fmail-approved amendments into `docs/tui-visual-polish-plan.md` before coding pass.

### 3) High: “every tab polished” goal not yet matched by current scope/state

Evidence:
- stated target in plan: polished every tab.
- logs golden still placeholder (`crates/forge-tui/tests/golden/layout/logs_80x24.txt`).
- fallback branch still emits generic placeholder (`crates/forge-tui/src/app.rs:3266`).

Impact:
- operator-perceived quality gap remains in a primary workflow tab.

Recommendation:
- make Logs polish a first-class work item, not implicit carry-over.
- explicitly define empty/loading/error/log-layer visual states for Logs.

### 4) Medium: no formal degradation tier model for narrow terminals

Evidence:
- current approach mostly ad-hoc truncation and clamp math.
- snapshot matrix exists, but no declarative degradation states by pane.
- fmail review asks for explicit breakpoints/fallback behavior.

Impact:
- 80x24 behavior remains fragile when visual density increases.

External benchmark:
- FrankenTUI specs use deterministic tier degradation with explicit warning codes (`docs/spec/mermaid-config.md`, tier downgrade flow).
- performance HUD explicitly defines tiny-terminal minimal fallback (`docs/performance-hud.md`).

Recommendation:
- adopt explicit per-pane tiers (`full`, `compact`, `minimal`) with deterministic triggers.

### 5) Medium: discoverability contract vulnerable during tab/footer modernization

Evidence:
- numeric tab switching core is documented and wired (`crates/forge-tui/src/keymap.rs`, help content in `crates/forge-tui/src/app.rs:3828`).
- polish direction removes visible numeric prefixes and truncates hint rail.

Impact:
- first-time operator navigation cost goes up.

External benchmark:
- FrankenTUI keybinding policy is explicit state machine + timeout contract (`docs/spec/keybinding-policy.md`).

Recommendation:
- keep at least one always-visible tab-switch affordance in shell (numbers in tabs OR guaranteed footer segment).
- treat discoverability as testable contract, not style preference.

### 6) Medium: missing decision-evidence loop for resize/degradation choices

Evidence:
- Forge has snapshot tests and perf gates, but no structured decision trace for resize/degrade choices.
- fmail discussions repeatedly debate these choices qualitatively.

External benchmark:
- FrankenTUI exposes deterministic resize decision JSONL + checksum (`docs/telemetry.md`, ResizeCoalescer evidence).

Recommendation:
- add lightweight in-repo evidence log for layout/degradation decisions under resize (test-only feature flag acceptable).

## Strategy delta (recommended v2)

### Phase 0: safety + contract freeze (must happen first)

1. Width-safe invariant:
- central helper for clamp/truncate; remove byte slicing in render code.
- add lint/check script: fail CI if render code uses `[..width]` slicing on user text.

2. Contract freeze doc:
- canonical shell signals: header chips, tab discoverability, footer minimum hints.
- canonical inbox identity signals: subject-first plus compact ID retained.

3. Update plan source:
- fold fmail amendments into `docs/tui-visual-polish-plan.md` before implementation.

### Phase 1: shell polish with contract preservation

- Header: remove static clutter; keep transient status (`mode`, `follow`, focus signal).
- Tabs: modern styling; keep deterministic discoverability cue.
- Footer: structured hint objects + priority-based truncation.

### Phase 2: pane polish with deterministic degradation tiers

- Overview/Runs/Inbox/MultiLogs/Logs each define:
  - `full` layout
  - `compact` layout
  - `minimal` layout
- explicit width triggers and fallback behavior per pane.

### Phase 3: evidence and gates

- retain snapshot matrix (`80x24`, `120x40`, `200x50`).
- add unicode stress snapshots and targeted text assertions.
- preserve perf gates (`docs/tui-705-performance-benchmark-suite-slo-gates.md`).
- add resize/degrade evidence logs for deterministic post-mortem.

### Phase 4: operator loop validation

- quick manual pass on real sessions:
  - dense multi-log cockpit
  - inbox conflict handling
  - runs->logs jump flow
  - command-palette pane-routing

## Immediate next actions (small, high leverage)

1. Update `docs/tui-visual-polish-plan.md` with already-agreed fmail amendments.
2. Open one implementation task for width-safe truncation sweep + tests.
3. Add explicit Logs-tab polish section + acceptance checks.
4. Define per-pane `full/compact/minimal` trigger table in plan doc.

## Net

Strong foundation shipped. Main risk now: polish pass reintroduces instability unless invariants/contracts are locked first. Focus first on width safety + contract alignment; then visual work can move fast with low regression risk.
