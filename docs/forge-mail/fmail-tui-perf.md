# fmail TUI perf harness (tags=perf)

Goal: reproducible perf smoke + baseline benchmarks for fmail TUI provider/view hot paths.

## Commands

- Perf smoke (budget checks; opt-in):
  - `make perf-smoke`
  - or: `env -u GOROOT -u GOTOOLDIR go test -tags=perf ./internal/fmailtui/... -run TestPerfSmokeBudgets -count=1`
- Benchmarks (capture baseline numbers):
  - `make perf-bench`
  - or: `env -u GOROOT -u GOTOOLDIR go test -tags=perf ./internal/fmailtui/perf -run '^$' -bench Perf -benchmem -count=1`

Note: Make targets unset `GOROOT`/`GOTOOLDIR` to avoid local toolchain env mismatch issues.
If your shell overrides `make` (e.g. zsh function), use `/usr/bin/make` or `command make`.

## Dataset

Synthetic mailbox generated in temp dir:
- topics: 200
- topic messages: 20 each (4k total)
- DM peers: 50
- DM messages: 20 each direction (2k total)

Total: ~6k messages, written as on-disk JSON (same format as real `.fmail` store).

## Budgets (default)

Budgets are enforced only in `tags=perf` tests (so normal `go test ./...` unaffected).

- Cold load:
  - `FileProvider.Topics()` + `DMConversations()` + `Search(text)` (first build): <= 350ms
- Refresh:
  - `FileProvider.Topics()` (warm/cache hit): <= 15ms
- Search latency:
  - `Search(text)` warm index: <= 35ms

## Tuning / slow machines

Scale budgets by a factor:
- `FM_PERF_BUDGET_SCALE=2 make perf-smoke`

## Comparing results over time

Suggested workflow:
1. `make perf-bench > build/fmailtui-perf-baseline.txt`
2. After perf changes: re-run and diff.
