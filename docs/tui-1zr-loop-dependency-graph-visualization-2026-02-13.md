# TUI-1ZR loop dependency graph visualization

## Scope
- Add a deterministic loop dependency graph core for visualization and operator triage.

## Changes
- Added `crates/forge-tui/src/loop_dependency_graph.rs`:
  - input model: `LoopDependencyInput`
  - computed graph model: `LoopDependencyGraph`, `LoopDependencyNode`
  - builder: `build_loop_dependency_graph(...)`
  - renderer: `render_loop_dependency_lines(...)`
  - cycle detection and longest-chain derivation
  - blocker/incoming/outgoing dependency metrics per loop
- Exported module in `crates/forge-tui/src/lib.rs`.

## Validation
- `cargo test -p forge-tui loop_dependency_graph::tests:: -- --nocapture`
- `cargo build -p forge-tui`
