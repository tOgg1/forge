# Semantic Incident Map Overlay (forge-4qy)

Date: 2026-02-13
Task: `forge-4qy`

## Scope implemented

Added `crates/forge-tui/src/semantic_incident_map.rs` and exported it from `crates/forge-tui/src/lib.rs`.

Implemented a deterministic graph model linking loop/run/inbox/failure entities:

- `IncidentSample`: per-incident input containing loop, run, inbox thread, failure signature, severity, mention weight, and correlated loops.
- `build_semantic_incident_map(samples)`: builds a deduplicated node/edge graph with severity upgrades and weighted link types.
- `IncidentNodeKind`: `loop`, `run`, `inbox`, `failure`.
- `IncidentEdgeKind`: `triggered`, `mentions`, `failed-with`, `correlated-loop`.
- `render_incident_map_rows(...)`: deterministic row output for panel/snapshot wiring.

Graph behavior:

- shared failures collapse to one failure node
- correlated loops become explicit graph links from failure nodes
- repeated node sightings upgrade to highest observed severity
- edge weights preserve mention/severity intensity

## Tests added

In `semantic_incident_map::tests`:

- graph includes all expected entity kinds
- shared failure signatures deduplicate into one node
- correlation edge creation checks
- deterministic row snapshot output

## Validation

Executed:

- `cargo fmt --all -- crates/forge-tui/src/semantic_incident_map.rs crates/forge-tui/src/lib.rs`
- `cargo test -p forge-tui semantic_incident_map::tests:: -- --nocapture`
