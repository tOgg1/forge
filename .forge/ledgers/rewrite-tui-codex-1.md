---
loop_id: 1e24f5b9-ca9f-481c-9148-087d7b2cfacb
loop_name: rewrite-tui-codex-1
repo_path: /Users/trmd/Code/oss--forge/repos/forge
created_at: 2026-02-12T10:59:19Z
---

# Loop Ledger: rewrite-tui-codex-1

## 2026-02-12T11:07:32Z

- run_id: 4040aa0c-f2b0-47f2-906c-f276577c211d
- loop_name: rewrite-tui-codex-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/swarm-tui-next-codex-continuous.md
- prompt_override: false
- started_at: 2026-02-12T10:59:24Z
- finished_at: 2026-02-12T11:07:32Z
- exit_code: 0

```
-            width,
-        );
-        frame.draw_text(0, 1, &subheader, TextRole::Muted);
+        if header_rows > 1 {
+            // Subheader line.
+            let subheader = truncate(
+                &format!(
+                    "layer:{}  pin:<space> clear:c  layout:m  page:,/. g/G  order:pinned first",
+                    self.log_layer().label(),
+                ),
+                width,
+            );
+            frame.draw_text(0, 1, &subheader, TextRole::Muted);
+        }
 
         // Grid of mini panes.
-        let header_rows = MULTI_HEADER_ROWS as usize;
-        let gap = MULTI_CELL_GAP.max(0) as usize;
+        let header_rows = header_rows as usize;
+        let gap = cell_gap.max(0) as usize;
         let mut index = 0;
 
         for row in 0..layout.rows as usize {
diff --git a/docs/tui-next-roadmap-2026-02-10.md b/docs/tui-next-roadmap-2026-02-10.md
index 90724c057795a55a252ae95b49b1eecd64086c61..97cdf6e46b9fb8a52ac9cdbd69841d54629c1272
--- a/docs/tui-next-roadmap-2026-02-10.md
+++ b/docs/tui-next-roadmap-2026-02-10.md
@@ -71,3 +71,4 @@
 - `forge-exd` delivered in `docs/tui-906-reference-plugins-extension-docs.md` with signed reference plugin bundle APIs, generated extension developer guide content, and permission safety lint warnings in `crates/forge-tui/src/extension_reference.rs`.
 - `forge-9hq` delivered in `docs/tui-801-theme-packs-semantic-slots.md` with curated semantic-slot theme packs, status/token-class mappings, and validated JSON import/export support in `crates/forge-tui/src/theme.rs`.
 - `forge-zzw` delivered in `docs/tui-802-accessibility-contrast-validator.md` with capability-aware contrast validation (`truecolor`/`ansi256`/`ansi16`), fail-fast violation reporting, and aggregate accessibility reports in `crates/forge-tui/src/theme.rs`.
+- `forge-d8z` delivered in `docs/tui-803-density-modes-focus-mode.md` with `comfortable/compact` density controls, deep focus mode (`Z`) for distraction-minimized debugging, and density-aware multi-log matrix fitting in `crates/forge-tui/src/app.rs` and `crates/forge-tui/src/multi_logs.rs`.

tokens used
361,638
`forge-d8z` claimed, implemented, reported as `blocked` (kept `in_progress`).

Changes shipped:
- density + focus model in `crates/forge-tui/src/app.rs:127` (`DensityMode`, `FocusMode`), key controls (`M`, `Z`), deep-focus render behavior, density-aware multi layout helpers, new tests.
- palette actions added in `crates/forge-tui/src/command_palette.rs:35` (`CycleDensityMode`, `ToggleFocusMode`) + registry/test updates.
- multi-log pane now uses dynamic density/focus layout params in `crates/forge-tui/src/multi_logs.rs:122`.
- docs added `docs/tui-803-density-modes-focus-mode.md:1`.
- roadmap updated `docs/tui-next-roadmap-2026-02-10.md:74`.

Validation:
- PASS `cargo fmt --check`
- FAIL workspace gates (pre-existing unrelated drift):
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - blocker file/lines: `crates/forge-agent/src/capability.rs:4`, `crates/forge-agent/src/capability.rs:114`, `crates/forge-agent/src/capability.rs:116`, `crates/forge-agent/src/capability.rs:120`, `crates/forge-agent/src/capability.rs:213`, `crates/forge-agent/src/capability.rs:214`, `crates/forge-agent/src/capability.rs:254`
- Scoped quality PASS:
  - `cargo clippy -p forge-tui --all-targets -- -D warnings`
  - `cargo test -p forge-tui` (all passing)

Protocol updates sent:
- `fmail send task` with progress + blocker details + requested unblock.
- `fmail send @forge-orchestrator "forge-d8z: blocked"`.

Requested unblock:
1. land/sync `forge-agent` type changes (`AgentRequestMode`, `SpawnAgentParams.requested_mode`, `SpawnAgentParams.allow_oneshot_fallback`), then rerun workspace gates.
```

## 2026-02-12T11:19:41Z

- run_id: 7941a0f1-80be-4ad5-ae96-62b9dd30475b
- loop_name: rewrite-tui-codex-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/swarm-tui-next-codex-continuous.md
- prompt_override: false
- started_at: 2026-02-12T11:08:02Z
- finished_at: 2026-02-12T11:19:41Z
- exit_code: 0

```
+
+Added tests in `crates/forge-tui/src/task_notes.rs` for:
+
+- required-field validation for notes/breadcrumbs
+- merged timeline ordering across note + breadcrumb events
+- related-reference retention
+- notes pane rendering with attribution and linkage
+- empty-pane hint rendering
+
+## Validation
+
+- `cargo fmt --check`
+- `cargo clippy --workspace --all-targets -- -D warnings`
+- `cargo test --workspace`
diff --git a/docs/tui-next-roadmap-2026-02-10.md b/docs/tui-next-roadmap-2026-02-10.md
index 8cdb5a36ab293a3081ecf22e48b2ee9984d8abe2..aaf1ca8aae0010512a534ce52e087c5b79ffc6c3
--- a/docs/tui-next-roadmap-2026-02-10.md
+++ b/docs/tui-next-roadmap-2026-02-10.md
@@ -53,6 +53,7 @@
 - `forge-jws` delivered in `docs/tui-602-claim-feed-conflicts.md` with claim timeline, conflict alerts, and resolution shortcuts in `crates/forge-tui/src/app.rs`.
 - `forge-73b` delivered in `docs/tui-501-swarm-template-library.md` with reusable `small/medium/full` swarm templates in `crates/forge-tui/src/swarm_templates.rs`.
 - `forge-nse` delivered in `docs/tui-603-handoff-snapshot-generator.md` with Inbox handoff snapshot generation (`h`) and compact package rendering in `crates/forge-tui/src/app.rs`.
+- `forge-daf` delivered in `docs/tui-604-shared-notes-breadcrumbs.md` with per-task shared notes, timestamped/attributed breadcrumbs, merged timeline rows, and notes-pane rendering helpers in `crates/forge-tui/src/task_notes.rs`.
 - `forge-rky` delivered in `docs/tui-502-controlled-ramp-wizard.md` with staged ramp-up and health-gate progression logic in `crates/forge-tui/src/swarm_templates.rs`.
 - `forge-k1s` delivered in `docs/tui-503-concurrency-governor.md` with concurrency governor + starvation-throttle recommendations in `crates/forge-tui/src/swarm_governor.rs`.
 - `forge-0q3` delivered in `docs/tui-504-dogpile-detector-redistribution.md` with duplicate-claim detection + redistribution action planning in `crates/forge-tui/src/swarm_dogpile.rs`.
@@ -73,3 +74,5 @@
 - `forge-9hq` delivered in `docs/tui-801-theme-packs-semantic-slots.md` with curated semantic-slot theme packs, status/token-class mappings, and validated JSON import/export support in `crates/forge-tui/src/theme.rs`.
 - `forge-zzw` delivered in `docs/tui-802-accessibility-contrast-validator.md` with capability-aware contrast validation (`truecolor`/`ansi256`/`ansi16`), fail-fast violation reporting, and aggregate accessibility reports in `crates/forge-tui/src/theme.rs`.
 - `forge-d8z` delivered in `docs/tui-803-density-modes-focus-mode.md` with `comfortable/compact` density controls, deep focus mode (`Z`) for distraction-minimized debugging, and density-aware multi-log matrix fitting in `crates/forge-tui/src/app.rs` and `crates/forge-tui/src/multi_logs.rs`.
+- `forge-m7a` delivered in `docs/tui-203-timeline-scrubber-heatmap.md` with timeline bucket aggregation, activity+error density heatmap rendering, ratio-based scrub seek, empty-bucket fallback, and stable anchored viewport math in `crates/forge-tui/src/timeline_scrubber.rs`.
+- `forge-110` delivered in `docs/tui-204-failure-jump-root-cause-focus.md` with first-failure jump detection, root-cause chain extraction, command-context linkage, and cause-line highlight metadata/jump helpers in `crates/forge-tui/src/failure_focus.rs`.

tokens used
197,801
Loop run result:

1. Closed `forge-m7a` (`TUI-203`).
- Added timeline scrubber model: `crates/forge-tui/src/timeline_scrubber.rs`.
- Docs: `docs/tui-203-timeline-scrubber-heatmap.md`.
- Validation passed: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`.

2. Closed `forge-110` (`TUI-204`).
- Added failure/root-cause focus model: `crates/forge-tui/src/failure_focus.rs`.
- Docs: `docs/tui-204-failure-jump-root-cause-focus.md`.
- Validation passed (same full gate).

3. Claimed `forge-daf` (`TUI-604`) and implemented, but blocked.
- Added shared notes/breadcrumb model: `crates/forge-tui/src/task_notes.rs`.
- Docs: `docs/tui-604-shared-notes-breadcrumbs.md`.
- Blocker posted to `fmail task` + `@forge-orchestrator`.
- Blocking command: `cargo clippy --workspace --all-targets -- -D warnings`
- Blocking files: `crates/forge-tui/src/multi_logs.rs:10`, `crates/forge-tui/src/multi_logs.rs:192`
- Reason: concurrent `forge-n5v` WIP references missing `render_compare_logs_pane` + unused imports.
- Per protocol: kept `forge-daf` `in_progress` (not closed).

Also updated roadmap/export wiring:
- `docs/tui-next-roadmap-2026-02-10.md`
- `crates/forge-tui/src/lib.rs`
- Plus minimal lint fix needed for workspace gate in `crates/forge-agent/src/lifecycle.rs`.
```

## 2026-02-12T11:32:57Z

- run_id: 3a37486a-3ae6-46c3-a417-77641a99c540
- loop_name: rewrite-tui-codex-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/swarm-tui-next-codex-continuous.md
- prompt_override: false
- started_at: 2026-02-12T11:20:11Z
- finished_at: 2026-02-12T11:32:57Z
- exit_code: 0

```
+
+## Validation
+
+- `cargo fmt --check`
+- `cargo clippy --workspace --all-targets -- -D warnings`
+- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
diff --git a/docs/tui-next-roadmap-2026-02-10.md b/docs/tui-next-roadmap-2026-02-10.md
index aaf1ca8aae0010512a534ce52e087c5b79ffc6c3..71aded7a88d0e1962161845981cbc1e8cf02e002
--- a/docs/tui-next-roadmap-2026-02-10.md
+++ b/docs/tui-next-roadmap-2026-02-10.md
@@ -49,6 +49,7 @@
 - `forge-8dc` delivered in `docs/tui-102-command-palette.md`, `crates/forge-tui/src/command_palette.rs`, and `Ctrl+P` integration in `crates/forge-tui/src/app.rs`.
 - `forge-3yh` delivered in `docs/tui-103-keymap-engine.md`, centralized keymap engine in `crates/forge-tui/src/keymap.rs`, and diagnostics integration in `crates/forge-tui/src/app.rs`.
 - `forge-exn` delivered in `docs/tui-301-fleet-selection-engine.md` with expressive id/name/repo/profile/pool/state/tag/stale filters and pre-action preview generation in `crates/forge-tui/src/fleet_selection.rs`.
+- `forge-5bh` delivered in `docs/tui-303-safety-policies-destructive-action-guardrails.md` with policy-aware blocking for destructive actions (protected pools/tags + batch thresholds), escalation hints, explicit confirmation handoff, and structured override audit entries in `crates/forge-tui/src/actions.rs`.
 - `forge-ezv` delivered in `docs/tui-601-fmail-inbox-panel.md` with Inbox tab state/render/actions in `crates/forge-tui/src/app.rs`.
 - `forge-jws` delivered in `docs/tui-602-claim-feed-conflicts.md` with claim timeline, conflict alerts, and resolution shortcuts in `crates/forge-tui/src/app.rs`.
 - `forge-73b` delivered in `docs/tui-501-swarm-template-library.md` with reusable `small/medium/full` swarm templates in `crates/forge-tui/src/swarm_templates.rs`.
@@ -62,6 +63,8 @@
 - `forge-8h3` delivered in `docs/tui-401-unified-fact-model.md` with canonical runs/tasks/queues/agents schema, derivation rules, and repository consistency checks in `crates/forge-tui/src/analytics_fact_model.rs`.
 - `forge-318` delivered in `docs/tui-403-blocker-graph-bottleneck-view.md` with dependency-edge normalization, impact-ranked bottlenecks, and actionable task drill-down links in `crates/forge-tui/src/blocker_graph.rs`.
 - `forge-350` delivered in `docs/tui-402-throughput-cycle-time-dashboards.md` with throughput/completion charts, cycle-time and queue-aging tables, and deterministic velocity summaries in `crates/forge-tui/src/analytics_dashboard.rs`.
+- `forge-1fx` delivered in `docs/tui-404-readiness-board-priority-risk-overlays.md` with project/epic filterable readiness-board rows, priority overlays, stale-risk and owner-gap signals, and deterministic risk-first ordering in `crates/forge-tui/src/readiness_board.rs`.
+- `forge-2er` delivered in `docs/tui-702-data-polling-pipeline-backpressure-jitter.md` with bounded polling queues, deterministic jittered poll intervals, backlog-driven backpressure penalties, and interactive-loop integration in `crates/forge-tui/src/polling_pipeline.rs` and `crates/forge-tui/src/bin/forge-tui.rs`.
 - `forge-r1d` delivered in `docs/tui-105-global-search-index.md` with incremental cross-entity indexing, repo/profile/tag filters, partial-match semantics, and relevance+recency ranking in `crates/forge-tui/src/global_search_index.rs`.
 - `forge-chf` delivered in `docs/tui-106-session-restore-delta-digest.md` with privacy-safe session snapshots, opt-out restore/persist controls, availability-aware restore fallbacks, and deterministic context delta digests in `crates/forge-tui/src/session_restore.rs`.
 - `forge-bx4` delivered in `docs/tui-104-layout-preset-system.md` with schema-versioned layout preset persistence/restoration, v1 migration path, corruption-resilient fallback/normalization, and effective layout application via `fit_pane_layout` in `crates/forge-tui/src/layout_presets.rs`.

tokens used
510,666
**Loop status**
- `forge-1fx` claimed, shipped, validated, closed.
- `forge-2er` claimed; implementation done; task kept `in_progress` (blocked gate).

**Shipped in this loop**
- `forge-1fx` (`TUI-404`) delivered:
  - `crates/forge-tui/src/readiness_board.rs`
  - `docs/tui-404-readiness-board-priority-risk-overlays.md`
  - `docs/tui-next-roadmap-2026-02-10.md`
- `forge-2er` (`TUI-702`) implementation landed:
  - `crates/forge-tui/src/polling_pipeline.rs`
  - `crates/forge-tui/src/bin/forge-tui.rs`
  - `crates/forge-tui/src/lib.rs`
  - `docs/tui-702-data-polling-pipeline-backpressure-jitter.md`
  - `docs/tui-next-roadmap-2026-02-10.md`

**Validation**
- `cargo fmt --check`: PASS
- `cargo clippy --workspace --all-targets -- -D warnings`: PASS
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`: FAIL (external drift)

**Current blocker (`forge-2er`)**
- Command: `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
- File/line: `crates/forge-cli/tests/completion_command_test.rs:8` (also `:16`, `:24`)
- Reason: completion output (`bash/zsh/fish`) diverges from goldens due concurrent `forge-cli` changes.
- Requested unblock posted on `fmail task`: sync completion generator output + golden fixtures (or revert completion output drift), then rerun gate.

**Protocol**
- `sv` + `fmail` used each iteration.
- Claims/progress/blockers/close posted to `task` + `@forge-orchestrator`.
```

