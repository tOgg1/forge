# TUI-922 cross-loop impact analysis

Task: `forge-gzg`  
Status: delivered

## Scope

- Detect ripple effects of shared-surface changes across active loops.
- Analyze proposed changes for:
  - crate public API symbols
  - shared config keys
  - migration symbols
- Produce impact matrix from source loop to affected loops.
- Recommend operator coordination actions:
  - pause critical active dependents
  - notify medium+ dependents
  - let race when no meaningful impact
- Generate fmail-ready coordination messages.

## Implementation

- New module: `crates/forge-tui/src/cross_loop_impact_analysis.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

### Core API

- `analyze_cross_loop_impact(...)`:
  - normalizes change/dependency inputs
  - matches overlapping symbols per component + interface kind
  - scores and classifies impact severity per loop
  - builds matrix rows and coordination actions
- `render_impact_matrix_rows(...)`: deterministic table-line rendering.
- `build_fmail_coordination_messages(...)`: deduplicated DM payloads for impacted loops.

## Validation

- `cargo fmt --all`
- `cargo test -p forge-tui cross_loop_impact_analysis::`
- `cargo build -p forge-tui --lib`
