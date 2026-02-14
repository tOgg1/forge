# forge-kp7: forge-ftui-adapter expect-used slice validation (2026-02-13)

## Summary
- Task scope (`expect-used` around frame buffer assertions in `crates/forge-ftui-adapter/src/lib.rs`) is already clean in current tree.
- No code changes required.

## Validation
- `cargo clippy -p forge-ftui-adapter --all-targets -- -D warnings`
- `cargo test -p forge-ftui-adapter --lib` (33 passed)
