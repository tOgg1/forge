# forge-0yx: forge-tui hero_widgets derivable_impls slice (2026-02-13)

## Scope
Fix clippy `derivable_impls` in `crates/forge-tui/src/hero_widgets.rs`.

## Changes
For `TrendDirection`:

- added `Default` derive
- marked `Flat` as `#[default]`
- removed manual `impl Default`

Formatted touched file.

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-tui/src/hero_widgets.rs
cargo test -p forge-tui --lib hero_widgets
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- hero-widgets tests passed (`14 passed`)
- clippy run with unrelated lint classes allowed still fails elsewhere, but `hero_widgets.rs` no longer appears in diagnostics
