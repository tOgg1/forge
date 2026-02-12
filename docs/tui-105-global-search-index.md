# TUI-105 global search index across loops, runs, tasks, and logs

Task: `forge-r1d`
Status: delivered

## Scope

- Added incremental in-memory search index for cross-entity lookup:
  - loops
  - runs
  - tasks
  - logs
- Supports repo/profile/tag/kind filters.
- Supports partial-match search semantics with deterministic relevance + recency ranking.

## Model

- `SearchDocument`: normalized index record (`id`, `kind`, `title`, `body`, `repo`, `profile`, `tags`, `updated_at`)
- `GlobalSearchIndex`:
  - `upsert` (incremental add/update)
  - `remove`
  - `search`
- Query contracts:
  - `SearchRequest`
  - `SearchFilter`
  - `SearchHit`

## Ranking and match rules

- Query tokenization is normalized lowercase alphanumeric.
- Partial-match semantics:
  - exact token match
  - prefix token match
  - substring token match
- Relevance scoring boosts:
  - ID/title exact and prefix matches
  - token and body hits
- Recency bonus:
  - decays over a 7-day window using `updated_at` vs request `now`.
- Final sort: `score DESC`, then `updated_at DESC`, then `id ASC`.

## Filtering

- `repo` exact normalized match.
- `profile` exact normalized match.
- `required_tags`: all required tags must be present.
- `kinds`: optional entity kind allow-list.

## Implementation

- New module: `crates/forge-tui/src/global_search_index.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
