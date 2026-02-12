# tui-next roadmap (2026-02-10)

Project:
- ID: `prj-v5pc07bf`
- Name: `tui-next`
- Goal: next-generation Forge TUI with premium logs, orchestration, analytics, and collaboration.

Program epic:
- `forge-k52` TUI-000 Epic: Forge Next-Gen TUI program

Domain epics:
- `forge-v67` TUI-100 Navigation, command palette, and workspace UX
- `forge-3t4` TUI-200 Logs intelligence and semantic rendering
- `forge-zad` TUI-300 Fleet control and safety rails
- `forge-gtx` TUI-400 Run and task analytics cockpit
- `forge-tf7` TUI-500 Swarm orchestration cockpit
- `forge-ty5` TUI-600 Collaboration and handoff flows
- `forge-er1` TUI-700 Reliability, replay, and performance
- `forge-vfd` TUI-800 Personalization and accessibility
- `forge-325` TUI-900 Plugin and extension platform

Included existing logs-highlighting epic:
- `forge-9m4` PAR-100 high-fidelity syntax highlighting (now in `tui-next`, under `forge-3t4`)

Counts snapshot:
- Open tasks in project: 81
- Ready tasks in project: 12

Priority model:
- P0: core interaction, logs core, fleet core, reliability/perf core
- P1: orchestration workflows, analytics core, collaboration core, log drill-down features
- P2: personalization and extension platform

Dependency spine:
- `forge-k52` blocks: `forge-v67`, `forge-3t4`, `forge-zad`, `forge-er1`
- `forge-v67` + `forge-3t4` feed analytics (`forge-gtx`)
- `forge-zad` + `forge-3t4` feed swarm cockpit (`forge-tf7`)
- `forge-er1` gates personalization (`forge-vfd`) and extensions (`forge-325`)

Suggested first spawn set:
- `forge-cey` TUI-101 IA
- `forge-xv4` TUI-201 multi-lane logs model
- `forge-exn` TUI-301 fleet selection engine
- `forge-qxw` TUI-701 incremental render engine
- `forge-5s3` PAR-101 logs corpus pack

Implementation notes:
- `forge-cey` delivered in `docs/tui-101-navigation-ia.md` and `crates/forge-tui/src/navigation_graph.rs`.
- `forge-8dc` delivered in `docs/tui-102-command-palette.md`, `crates/forge-tui/src/command_palette.rs`, and `Ctrl+P` integration in `crates/forge-tui/src/app.rs`.
- `forge-3yh` delivered in `docs/tui-103-keymap-engine.md`, centralized keymap engine in `crates/forge-tui/src/keymap.rs`, and diagnostics integration in `crates/forge-tui/src/app.rs`.
- `forge-exn` delivered in `docs/tui-301-fleet-selection-engine.md` with expressive id/name/repo/profile/pool/state/tag/stale filters and pre-action preview generation in `crates/forge-tui/src/fleet_selection.rs`.
- `forge-s1r` delivered in `docs/tui-302-bulk-action-planner-stop-scale-msg-inject.md` with dry-run bulk planning for `stop/scale/msg/inject`, conflict diagnostics, rollback hints, and transparent queued command previews in `crates/forge-tui/src/bulk_action_planner.rs`.
- `forge-5bh` delivered in `docs/tui-303-safety-policies-destructive-action-guardrails.md` with policy-aware blocking for destructive actions (protected pools/tags + batch thresholds), escalation hints, explicit confirmation handoff, and structured override audit entries in `crates/forge-tui/src/actions.rs`.
- `forge-yj4` delivered in `docs/tui-306-emergency-safe-stop-all-workflow.md` with one-key emergency safe-stop workflow modeling, scope preview filters, staged stop execution state, and post-stop integrity checks/escalation hints in `crates/forge-tui/src/emergency_safe_stop.rs`.
- `forge-ezv` delivered in `docs/tui-601-fmail-inbox-panel.md` with Inbox tab state/render/actions in `crates/forge-tui/src/app.rs`.
- `forge-jws` delivered in `docs/tui-602-claim-feed-conflicts.md` with claim timeline, conflict alerts, and resolution shortcuts in `crates/forge-tui/src/app.rs`.
- `forge-73b` delivered in `docs/tui-501-swarm-template-library.md` with reusable `small/medium/full` swarm templates in `crates/forge-tui/src/swarm_templates.rs`.
- `forge-nse` delivered in `docs/tui-603-handoff-snapshot-generator.md` with Inbox handoff snapshot generation (`h`) and compact package rendering in `crates/forge-tui/src/app.rs`.
- `forge-daf` delivered in `docs/tui-604-shared-notes-breadcrumbs.md` with per-task shared notes, timestamped/attributed breadcrumbs, merged timeline rows, and notes-pane rendering helpers in `crates/forge-tui/src/task_notes.rs`.
- `forge-rky` delivered in `docs/tui-502-controlled-ramp-wizard.md` with staged ramp-up and health-gate progression logic in `crates/forge-tui/src/swarm_templates.rs`.
- `forge-k1s` delivered in `docs/tui-503-concurrency-governor.md` with concurrency governor + starvation-throttle recommendations in `crates/forge-tui/src/swarm_governor.rs`.
- `forge-0q3` delivered in `docs/tui-504-dogpile-detector-redistribution.md` with duplicate-claim detection + redistribution action planning in `crates/forge-tui/src/swarm_dogpile.rs`.
- `forge-f1z` delivered in `docs/tui-505-quant-qual-stop-condition-monitor.md` with quant/qual stop-signal status, threshold surfacing, time-to-trigger, and mismatch reasoning in `crates/forge-tui/src/swarm_stop_monitor.rs`.
- `forge-5mw` delivered in `docs/tui-506-wind-down-workflow-reconciliation.md` with graceful stop sequencing, stale-check/ledger-sync gates, and closure summary reconciliation in `crates/forge-tui/src/swarm_wind_down.rs`.
- `forge-8h3` delivered in `docs/tui-401-unified-fact-model.md` with canonical runs/tasks/queues/agents schema, derivation rules, and repository consistency checks in `crates/forge-tui/src/analytics_fact_model.rs`.
- `forge-318` delivered in `docs/tui-403-blocker-graph-bottleneck-view.md` with dependency-edge normalization, impact-ranked bottlenecks, and actionable task drill-down links in `crates/forge-tui/src/blocker_graph.rs`.
- `forge-350` delivered in `docs/tui-402-throughput-cycle-time-dashboards.md` with throughput/completion charts, cycle-time and queue-aging tables, and deterministic velocity summaries in `crates/forge-tui/src/analytics_dashboard.rs`.
- `forge-1fx` delivered in `docs/tui-404-readiness-board-priority-risk-overlays.md` with project/epic filterable readiness-board rows, priority overlays, stale-risk and owner-gap signals, and deterministic risk-first ordering in `crates/forge-tui/src/readiness_board.rs`.
- `forge-mdc` delivered in `docs/tui-405-next-best-task-recommendation-engine.md` with operator-context-aware next-task ranking using priority/readiness/dependency/ownership/context scoring and explainable recommendation reasons in `crates/forge-tui/src/task_recommendation.rs`.
- `forge-2er` delivered in `docs/tui-702-data-polling-pipeline-backpressure-jitter.md` with bounded polling queues, deterministic jittered poll intervals, backlog-driven backpressure penalties, and interactive-loop integration in `crates/forge-tui/src/polling_pipeline.rs` and `crates/forge-tui/src/bin/forge-tui.rs`.
- `forge-r1d` delivered in `docs/tui-105-global-search-index.md` with incremental cross-entity indexing, repo/profile/tag filters, partial-match semantics, and relevance+recency ranking in `crates/forge-tui/src/global_search_index.rs`.
- `forge-chf` delivered in `docs/tui-106-session-restore-delta-digest.md` with privacy-safe session snapshots, opt-out restore/persist controls, availability-aware restore fallbacks, and deterministic context delta digests in `crates/forge-tui/src/session_restore.rs`.
- `forge-bx4` delivered in `docs/tui-104-layout-preset-system.md` with schema-versioned layout preset persistence/restoration, v1 migration path, corruption-resilient fallback/normalization, and effective layout application via `fit_pane_layout` in `crates/forge-tui/src/layout_presets.rs`.
- `forge-cpt` delivered in `docs/tui-901-extension-api-custom-panels.md` with stable custom-panel descriptor/lifecycle/render contracts, read-only vs interactive event handling, and registry/session host APIs in `crates/forge-tui/src/extension_api.rs`.
- `forge-j52` delivered in `docs/tui-902-extension-api-custom-actions.md` with extension action registration contracts, command/permission validation, audit metadata trail, and `PaletteActionId::Custom` export for command palette integration in `crates/forge-tui/src/extension_actions.rs`.
- `forge-4jp` delivered in `docs/tui-903-extension-sandbox-permissions.md` with explicit-grant sandbox enforcement for filesystem/process intents, capability restriction checks, allowlist/path-guard policy evaluation, and typed audit decisions in `crates/forge-tui/src/extension_sandbox.rs`.
- `forge-7cn` delivered in `docs/tui-904-extension-event-bus-schema-versioning.md` with versioned plugin event envelopes, per-kind schema compatibility declarations, dispatch skip reasons, and per-plugin event inbox/drain flow in `crates/forge-tui/src/extension_event_bus.rs`.
- `forge-axy` delivered in `docs/tui-905-plugin-packaging-discovery-lifecycle.md` with plugin package schema contracts, trusted-signer signature verification, discovery/install/uninstall lifecycle controls, host API compatibility checks, and lifecycle audit events in `crates/forge-tui/src/extension_package_manager.rs`.
- `forge-exd` delivered in `docs/tui-906-reference-plugins-extension-docs.md` with signed reference plugin bundle APIs, generated extension developer guide content, and permission safety lint warnings in `crates/forge-tui/src/extension_reference.rs`.
- `forge-9hq` delivered in `docs/tui-801-theme-packs-semantic-slots.md` with curated semantic-slot theme packs, status/token-class mappings, and validated JSON import/export support in `crates/forge-tui/src/theme.rs`.
- `forge-zzw` delivered in `docs/tui-802-accessibility-contrast-validator.md` with capability-aware contrast validation (`truecolor`/`ansi256`/`ansi16`), fail-fast violation reporting, and aggregate accessibility reports in `crates/forge-tui/src/theme.rs`.
- `forge-d8z` delivered in `docs/tui-803-density-modes-focus-mode.md` with `comfortable/compact` density controls, deep focus mode (`Z`) for distraction-minimized debugging, and density-aware multi-log matrix fitting in `crates/forge-tui/src/app.rs` and `crates/forge-tui/src/multi_logs.rs`.
- `forge-m7a` delivered in `docs/tui-203-timeline-scrubber-heatmap.md` with timeline bucket aggregation, activity+error density heatmap rendering, ratio-based scrub seek, empty-bucket fallback, and stable anchored viewport math in `crates/forge-tui/src/timeline_scrubber.rs`.
- `forge-110` delivered in `docs/tui-204-failure-jump-root-cause-focus.md` with first-failure jump detection, root-cause chain extraction, command-context linkage, and cause-line highlight metadata/jump helpers in `crates/forge-tui/src/failure_focus.rs`.
