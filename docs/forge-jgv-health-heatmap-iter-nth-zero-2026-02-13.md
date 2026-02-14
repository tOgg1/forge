# forge-jgv: health_heatmap_timeline iter_nth_zero slice (2026-02-13)

## Scope
Fix clippy `iter_nth_zero` in `crates/forge-tui/src/health_heatmap_timeline.rs` tests.

## Changes
Replaced:

- `chars().nth(0)` with `chars().next()`

in the ranked heatmap test assertion.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib builds_ranked_cross_loop_heatmap
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::needless_return \
  -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused health-heatmap test passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
