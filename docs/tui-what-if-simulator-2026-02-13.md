# What-If Simulator (forge-s0e)

Date: 2026-02-13
Task: `forge-s0e`

## Scope implemented

Added `crates/forge-tui/src/what_if_simulator.rs` and exported it from `crates/forge-tui/src/lib.rs`.

Implemented deterministic stop/scale impact simulation primitives:

- `LoopThroughputSample`: baseline loop capacity sample (`avg_run_secs`, `success_rate`, `queue_depth`, `running_agents`).
- `WhatIfAction`: action set for stop/resume/scale/inject-queue/success-rate overrides.
- `simulate_what_if(samples, actions)`: computes projected queue/throughput/ETA deltas per loop.
- `LoopWhatIfProjection`: baseline vs projected queue depth, throughput, ETA, and impact label (`improved`, `degraded`, `blocked`, etc.).
- `render_projection_rows(...)`: deterministic text rows for panel/snapshot rendering.

Model behavior highlights:

- clamps scaled agent counts at zero
- supports queue load injection for pressure testing
- treats zero throughput with non-zero queue as blocked ETA
- sorts output rows by loop ID for deterministic snapshots

## Tests added

In `what_if_simulator::tests`:

- stop action projects blocked ETA
- scale-up improves throughput and queue clear ETA
- large negative scale clamps at zero agents
- deterministic row snapshot for mixed action set

## Validation

Executed:

- `cargo fmt --all -- crates/forge-tui/src/what_if_simulator.rs crates/forge-tui/src/lib.rs`
- `cargo test -p forge-tui what_if_simulator::tests:: -- --nocapture`

Result: blocked by unrelated concurrent `forge-cli` compile failures in `crates/forge-cli/src/profile.rs` (`detect_profile_init` and `instantiate_profiles_from_detection` unresolved).
