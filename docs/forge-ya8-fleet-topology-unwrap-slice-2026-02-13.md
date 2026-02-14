# forge-ya8 - forge-tui fleet_topology_graph unwrap-used slice

Date: 2026-02-13
Task: `forge-ya8`
Scope: `crates/forge-tui/src/fleet_topology_graph.rs`

## Change

- Replaced test `unwrap()` in `focus_view_sorts_neighbors_by_intensity_desc` with explicit `match` + panic context.

## Validation

```bash
cargo test -p forge-tui --lib fleet_topology_graph::tests::focus_view_sorts_neighbors_by_intensity_desc
rg -n "unwrap\\(" crates/forge-tui/src/fleet_topology_graph.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::unwrap_used 2>&1 | rg 'fleet_topology_graph.rs' || true
```

Result:
- Targeted test passed.
- No `unwrap(` remains in this file.
- No `clippy::unwrap_used` diagnostics emitted for this file.

