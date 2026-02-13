# Loop Dependency Graph Visualization (forge-1zr)

Date: 2026-02-13
Task: `forge-1zr`

## Scope implemented

Added a loop-dependency graph model in `crates/forge-tui/src/blocker_graph.rs` to support TUI visualization planning:

- `LoopDependencySample`: loop state + upstream dependencies + optional subtree key.
- `build_loop_dependency_graph_view(...)`:
  - normalized dependency graph (upstream/downstream edges)
  - critical path extraction (root -> failing loop)
  - failure propagation paths (failing loop -> transitive impacted loops)
  - collapsible subtree compaction with representative loop + hidden count
- `render_loop_dependency_rows(...)`: deterministic ASCII rows suitable for panel rendering and snapshots.

## Model outputs

`LoopDependencyGraphView` includes:

- `nodes`: depth, upstream/downstream neighbors, critical-path index, collapsed member count
- `edges`: directed loop dependency edges
- `critical_path`: longest upstream path into focused/default failing loop
- `propagation_paths`: per-failing-loop downstream impact fan-out
- `collapsed_subtrees`: summary for collapsed subtree groups

## Test coverage added

In `blocker_graph::tests`:

- critical path selection for failing loop focus
- downstream failure propagation fan-out
- subtree collapse behavior + representative node accounting
- deterministic render row snapshot for graph rows

## Notes

Validation command attempted:

- `cargo test -p forge-tui blocker_graph::tests::`

Current workspace has unrelated compile failures in `crates/forge-cli/src/workflow.rs` (`Command`/`Stdio` unresolved), which blocked execution of new tests in this run.
