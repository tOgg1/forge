# forge-079 - forge-tui navigation_graph derivable_impls slice

Date: 2026-02-13
Task: `forge-079`
Scope: `crates/forge-tui/src/navigation_graph.rs`

## Change

- Replaced manual `Default` implementation for `ZoomSpatialAnchor` with `#[derive(Default)]`.

## Validation

```bash
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::derivable_impls
cargo test -p forge-tui --lib navigation_graph::tests
```

Result: both commands passed.

