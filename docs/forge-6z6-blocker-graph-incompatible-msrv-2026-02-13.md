# forge-6z6: blocker_graph incompatible_msrv slice (2026-02-13)

## Scope
Fix clippy `incompatible_msrv` (`Option::is_none_or`, stable 1.82) in `crates/forge-tui/src/blocker_graph.rs` for MSRV 1.81.

## Changes
Replaced three `is_none_or(...)` usages with `map_or(...)` equivalents:

- root detection in `compute_loop_depths`
- depth update condition in `compute_loop_depths`
- best-state selection in `aggregate_collapsed_states`

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib blocker_graph
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- blocker_graph focused suite passed (`8 passed`)
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
