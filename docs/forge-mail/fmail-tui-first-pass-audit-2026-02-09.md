# fmail TUI First-Pass Audit (2026-02-09)

Scope: first perf/UX pass based on current repo state.

## What is slow now

- Multiple views trigger full reloads on short ticks or message events:
  - `internal/fmailtui/timeline_view.go`
  - `internal/fmailtui/topics_view.go`
  - `internal/fmailtui/dashboard_view.go`
  - `internal/fmailtui/stats_view.go`
  - `internal/fmailtui/graph_view.go`
- `topics_view` computes unread/hot counts by repeatedly loading full topic/DM message slices.
- `timeline_view` aggregates by loading all topics and all DMs repeatedly, then filtering in-memory.
- Provider defaults are aggressive:
  - `internal/fmailtui/data/provider.go`: cache TTL `500ms`, poll interval `100ms`.
- `FileProvider` metadata paths still require broad scans:
  - `Topics()` computes topic metadata from message scans.
  - `DMConversations()` scans DM dirs and loads message lists.
  - `Subscribe()` polls frequently and scans directories/files each cycle.
- Search index TTL is short for a file-store workload:
  - `internal/fmailtui/data/search_index.go`: `2s`.

## Probe numbers (current local mailbox)

Mailbox size in this repo snapshot:
- topics json files: ~221
- dm json files: ~103

Local probe (`go run tmp_perf_fmailtui.go`):
- `Topics first`: ~10.2ms
- `Topics cached`: ~0ms
- `Messages first task`: ~0.03ms
- `DM conversations`: ~3.7ms
- `Search text first`: ~1.36ms
- `Search text cached idx`: ~0.10ms

Note: single-call timings are acceptable at current mailbox size; user-facing slowness is likely from repeated full reload patterns + frequent polling/ticks + repeated cross-topic scans per view refresh.

## First-pass plan

- Add incremental metadata/cache index in provider; avoid full scans on every view load.
- Reduce redundant load paths in dashboard/topics/timeline.
- Introduce bounded windows and lazy fetch in timeline/topics.
- Tune provider defaults and make them config-driven.
- Add perf harness + benchmark/trace commands with target budgets.
- Run a UI polish pass after perf floor is fixed (layout, readability, key paths).

## Update 2026-02-09 (forge-w6k)

- Timeline load path now uses bounded page reads (`Limit` + `Until`) and lazy older-page fetch near top-of-window navigation.
- Timeline tick loop no longer triggers full provider reload every 2 seconds; live updates come from `Subscribe()`.
- File provider now supports bounded directory reads for topic/DM message retrieval and narrows DM conversation reads to relevant DM dirs (`viewer` + `peer`) instead of scanning all DM dirs.
- Added regression tests:
  - `internal/fmailtui/timeline_view_test.go`: lazy paging + no tick reload regression.
  - `internal/fmailtui/data/file_provider_test.go`: unrelated corrupt DM dir does not break target conversation load.
- Validation blocker outside task scope: package `internal/fmailtui/dashboard_view.go` currently references missing methods (`shouldRefresh`, `applyTopicsSnapshot`, `applyIncoming`), causing `go test ./...` to fail before timeline package tests can run.
