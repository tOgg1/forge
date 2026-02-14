# Forge YTW Power Features Backlog (2026-02-13)

Epic: `forge-ytw`
Parent epic: `forge-yms`
Scope: post-cutover power-user capabilities for operator speed, incident depth, and automation.

## Prioritization Heuristic

Use this ordering signal for execution:

1. Operator cycle-time reduction in active incidents.
2. Dependency unlocks for other features.
3. Low integration risk on current FrankenTUI shell.
4. Evidence/export value for handoff and postmortems.

## Wave 1 (High-Leverage P2)

Target: ship first 8-10 capabilities with immediate incident ROI.

- `forge-mce` Universal fuzzy switcher (loop/run/thread/action).
- `forge-7pg` Notification center (snooze/escalate/ack).
- `forge-fs8` Triage score queue.
- `forge-h84` Cross-loop health heatmap timeline.
- `forge-rsr` Run output semantic diff view.
- `forge-qth` Playbook runner panel.
- `forge-m38` Multi-select bulk action planner + dry-run.
- `forge-gwc` Typed-reason destructive confirms.
- `forge-343` Pinned log lines and bookmarks.
- `forge-575` Regex log search with live highlights.

Execution order recommendation:

1. `forge-mce`
2. `forge-fs8`
3. `forge-7pg`
4. `forge-h84`
5. `forge-rsr`
6. `forge-m38`
7. `forge-gwc`
8. `forge-qth`
9. `forge-343`
10. `forge-575`

## Wave 2 (Advanced P2)

Target: richer analysis + automation after Wave 1 base is stable.

- `forge-ppm` Claim conflict predictor.
- `forge-rhm` Alert-rule DSL panel.
- `forge-rrr` Event hook automation.
- `forge-xep` Predictive queue ETA estimator.
- `forge-k96` Root cause waterfall visualization.
- `forge-js5` Postmortem auto-draft export.
- `forge-a50` Named workspace snapshots + restore.
- `forge-6ef` Live layout inspector + perf HUD.
- `forge-b4h` Degradation policy tuner.
- `forge-txz` Multi-node compare split view.
- `forge-gg8` Throughput gauge and rate meter.
- `forge-s63` Timeline swim lanes.
- `forge-1zr` Loop dependency graph visualization.
- `forge-tw6` Cost/resource tracker panel.
- `forge-h54` Scheduled actions.
- `forge-y10` Agent presence radar.
- `forge-cta` Keyboard macro record and replay.

## Wave 3 (Exploratory P3)

Target: collaborative and high-complexity surfaces.

- `forge-mry` War room mode.
- `forge-ws7` Shared annotations on runs/logs.
- `forge-rdh` Shareable dashboard snapshot URL.
- `forge-f5y` Tmux-aware integration.
- `forge-bch` Inline embedded terminal mode.
- `forge-npb` Picture-in-picture floating panels.
- `forge-aj9` Raw PTY attach mode.
- `forge-s0e` What-if simulator (stop/scale impact).
- `forge-4qy` Semantic incident map overlay.
- `forge-c52` Synchronized incident replay scrubber.
- `forge-abd` Plugin extension panel.

## Dependency Notes

- `forge-rhm` should precede `forge-rrr` (rule definition before hook automation).
- `forge-gwc` should precede wider multi-action rollouts (`forge-m38`, `forge-qth`).
- `forge-a50` should precede `forge-rdh` and `forge-mry` for stable session/state sharing.
- `forge-4qy` should precede `forge-c52` for reusable incident model primitives.
- `forge-mce` should be treated as cross-feature navigation substrate for all waves.

## Delivery Guardrails

- Keep WIP <= 3 power tasks in parallel.
- Require regression tests per feature module.
- Prefer deterministic snapshot tests for panel output.
- Track cycle-time delta before/after Wave 1 in operator drills.
