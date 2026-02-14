# forge-kp7: forge-ftui-adapter expect-used slice (2026-02-13)

## Scope
Fix clippy `expect_used` callsites in `crates/forge-ftui-adapter/src/lib.rs` upstream primitive tests.

## Changes
Replaced two buffer cell `.expect(...)` usages with explicit `match` handling in:

- `forge_badge_applies_theme_token_style`
- `forge_statusline_and_table_render_smoke`

Formatted touched file.

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-ftui-adapter/src/lib.rs
cargo clippy -p forge-ftui-adapter --all-targets -- -D warnings
```

Results:

- clippy passed for `forge-ftui-adapter`
