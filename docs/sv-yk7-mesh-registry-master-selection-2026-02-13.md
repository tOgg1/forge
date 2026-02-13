# sv-yk7 - Mesh registry + master selection (2026-02-13)

## Scope shipped
- Added `forge mesh` command family:
  - `forge mesh status`
  - `forge mesh promote <node-id> [--endpoint <addr>]`
  - `forge mesh demote <node-id>`
- Added file-backed mesh registry store:
  - `crates/forge-cli/src/mesh.rs`
  - persisted at `${FORGE_DATA_DIR}/mesh/registry.json` (via runtime path resolution)
- Added deterministic master selection behavior:
  - `promote` sets active master node id
  - `demote` clears master when demoting current master
  - status always reports active master and registered nodes

## Tests added
- `mesh::tests::status_defaults_to_empty_mesh_registry`
- `mesh::tests::promote_sets_active_master_and_registers_node`
- `mesh::tests::demote_clears_master_when_target_is_active_master`
- `mesh::tests::demote_unknown_node_fails`
- `mesh::tests::promote_updates_endpoint_for_existing_node`
- `mesh::tests::help_renders_usage`
- root wiring check:
  - `lib::tests::mesh_module_is_accessible`

## Validation
```bash
cargo test -p forge-cli --lib mesh::tests:: -- --nocapture
cargo test -p forge-cli --lib tests::mesh_module_is_accessible -- --nocapture
cargo build -p forge-cli
```
