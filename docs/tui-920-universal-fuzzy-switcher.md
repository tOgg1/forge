# TUI-920 universal fuzzy switcher

Task: `forge-mce`  
Status: delivered (validation currently blocked by unrelated in-progress `app.rs` compile errors)

## Scope

- Single fuzzy switch surface across:
  - loops
  - runs
  - inbox threads
  - actions
- Deterministic ranking with context/recency/usage bias.

## Implementation

- New module: `crates/forge-tui/src/universal_switcher.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core API:

- `UniversalSwitcher::ingest_palette_actions(...)`
- `UniversalSwitcher::ingest_search_hits(...)`
- `UniversalSwitcher::upsert_thread(...)`
- `UniversalSwitcher::search(query, context, limit)`
- `UniversalSwitcher::record_use(...)`

Core model:

- `SwitcherItemKind`
- `SwitcherTarget`
- `SwitcherItem`
- `SwitcherContext`
- `SwitcherMatch`
- `SwitcherSearchResult`

Ranking:

- query fuzzy score (exact/prefix/substring/ordered-subsequence)
- context boosts (`preferred_tab`, tab-kind default, selected-loop requirement)
- recency bonus (time-decayed by `updated_at`)
- usage bonus (`record_use` recency + count)

## Regression tests

Added in `crates/forge-tui/src/universal_switcher.rs`:

- mixed-entity search (loop/run/thread/action)
- selection-gated action filtering
- usage-bias promotion
- task-hit -> thread-target mapping

## Validation

Attempted:

- `cargo test -p forge-tui universal_switcher -- --nocapture`
- `cargo build -p forge-tui`

Blocked by unrelated current compile errors in `crates/forge-tui/src/app.rs`
from in-progress work (`let chains` edition mismatch + temporary borrow/type errors).
