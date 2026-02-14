# TUI-W50 agent performance profiler core

## Scope
- Add flame-style time allocation aggregation for agent activity profiling.

## Changes
- Added `crates/forge-tui/src/agent_performance_profiler.rs`:
  - `ProfilerSpan` input model (`agent_id`, `stack`, `duration_ms`)
  - `FlameNode` aggregate tree model
  - `build_flame_tree(...)` path aggregation
  - `summarize_agent_time(...)` per-agent totals + dominant frame
  - `render_flame_lines(...)` text flame output with relative bars
- Exported module in `crates/forge-tui/src/lib.rs`.

## Validation
- `cargo test -p forge-tui agent_performance_profiler::tests:: -- --nocapture`
- `cargo build -p forge-tui`
