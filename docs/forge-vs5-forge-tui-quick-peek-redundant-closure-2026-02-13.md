# forge-vs5: forge-tui quick_peek redundant_closure slice (2026-02-13)

## Scope
Fix clippy `redundant_closure` in `crates/forge-tui/src/quick_peek_popups.rs`.

## Changes
Replaced:

- `.map(|line| sanitize_inline(line))`

with:

- `.map(sanitize_inline)`

Formatted touched file.

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-tui/src/quick_peek_popups.rs
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::derivable_impls \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
cargo test -p forge-tui --lib loop_peek_contains_health_task_and_output
```

Results:

- clippy run with unrelated lints allowed passed (`CLIPPY_EXIT:0`)
- focused quick-peek unit test passed
