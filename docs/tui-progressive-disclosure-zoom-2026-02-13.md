# Progressive Disclosure Zoom (forge-sd4)

Date: 2026-02-13
Task: `forge-sd4`

## Scope implemented

Added semantic zoom primitives in `crates/forge-tui/src/navigation_graph.rs`:

- `ZoomLayer`: canonical abstraction stack (`Fleet` -> `Group` -> `Loop` -> `Task` -> `Diff`).
- `ZoomCommand`: directional and absolute zoom intents (`In`, `Out`, `Set(layer)`).
- `ZoomSpatialAnchor`: stable location payload (fleet cell + cluster/loop/task identifiers).
- `SemanticZoomState`: active layer + canonical zoom percent + spatial anchor.
- `SemanticZoomTransition`: transition metadata for UI status surfaces.

## Transition behavior

- `apply_semantic_zoom(state, command)`:
  - applies layer step with clamp at bounds
  - maps each layer to a canonical percent (`20/40/60/80/100`)
  - preserves anchor payload across transitions
  - emits `detail_hint` for rendering/status copy
- `zoom_layer_for_percent(percent)` maps arbitrary percent to semantic layer bands.
- `semantic_zoom_status_rows(state, max_rows)` emits deterministic status rows for panel wiring/snapshots.

## Tests added

In `navigation_graph::tests`:

- step-in/step-out clamping to fleet/diff bounds
- anchor preservation across direct layer jumps
- semantic percent-band mapping snapshots
- status row snapshot output

## Validation

Executed:

- `cargo fmt --all -- crates/forge-tui/src/navigation_graph.rs`
- `cargo test -p forge-tui navigation_graph::tests:: -- --nocapture`
