# forge-xfv: cross_loop_impact_analysis field_reassign_with_default slice (2026-02-13)

## Scope
Fix clippy `field_reassign_with_default` in `crates/forge-tui/src/cross_loop_impact_analysis.rs` test.

## Changes
Replaced:

- mutable default + field assignment

with:

- struct update initializer:
  `ImpactPolicy { critical_score: 60, ..ImpactPolicy::default() }`

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib includes_pause_action_for_critical_active_dependents
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::iter_nth_zero -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused cross-loop-impact test passed
- clippy run still fails elsewhere, but `cross_loop_impact_analysis.rs` no longer appears in diagnostics
