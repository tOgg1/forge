# forge-8ys: motion_grammar unwrap-used validation (2026-02-13)

## Scope
Validate motion_grammar unwrap-used slice after takeover.

## Findings
No code changes required:

- `crates/forge-tui/src/motion_grammar.rs` contains no `expect/unwrap` callsites.
- test module is currently feature-gated in this build; focused filter matched zero tests.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib reduced_motion_suppresses_triggers
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused test filter: 0 matched (feature-gated tests)
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
- `motion_grammar.rs` absent from diagnostics
