# forge-pr7 - forge-tui loop_dependency_graph expect-used slice

Date: 2026-02-13
Task: `forge-pr7`
Scope: `crates/forge-tui/src/loop_dependency_graph.rs`

## Change

- Replaced test `expect("loop-b")` with explicit `match` + panic context in `builds_edges_and_blocker_counts_for_known_dependencies`.

## Validation

```bash
cargo test -p forge-tui --lib loop_dependency_graph::tests::builds_edges_and_blocker_counts_for_known_dependencies
rg -n "expect\\(" crates/forge-tui/src/loop_dependency_graph.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'loop_dependency_graph.rs' || true
```

Result:
- Targeted test passed.
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.

