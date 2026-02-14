# forge-ecp - Fleet Topology Graph (2026-02-13)

## Scope shipped
- Added `crates/forge-tui/src/fleet_topology_graph.rs` with a deterministic topology model for loop relationships.
- Relationship edge types supported:
  - shared files
  - fmail communication
  - dependency links
  - crate ownership clusters
- Node health classification implemented (`ok`, `warn`, `crit`, `unknown`) from loop state, queue pressure, and error signal.
- Added interaction helpers needed by graph UX:
  - edge filtering by type (`TopologyEdgeFilter`)
  - focus context extraction (`focus_loop_topology`)
  - drag/reposition with bounds clamping (`drag_topology_node`)
- Added text renderer helper (`render_fleet_topology_lines`) with:
  - summary header
  - focus summary
  - cluster lines
  - weighted edge lines (`=` thickness from intensity)
  - node rows (health + cluster + position)

## Tests added
- `builds_edges_across_shared_files_fmail_dependency_and_crate_clusters`
- `classifies_health_with_state_queue_and_error_signals`
- `edge_filter_returns_only_enabled_kinds`
- `focus_view_sorts_neighbors_by_intensity_desc`
- `drag_clamps_node_position_to_bounds`
- `render_lines_include_focus_clusters_and_edges`

## Validation commands
```bash
cargo test -p forge-tui fleet_topology_graph::tests:: -- --nocapture
cargo build -p forge-tui
```
