# sv-0ze: Profile catalog + mesh status reporting

Date: 2026-02-13
Task: `sv-0ze`

## Scope delivered

- Extended mesh registry model with per-node profile auth map:
  - `MeshNode.profile_auth: { <profile_id>: <ok|expired|missing> }`
- Added profile catalog summary surface in mesh status:
  - total profiles
  - harness counts
  - canonical profile IDs (`CC1`, `Codex1`, `OC1`, ...)
- Added per-node and aggregate auth reporting:
  - per node: `profiles_total`, `auth_ok`, `auth_expired`, `auth_missing`
  - mesh totals: `ok`, `expired`, `missing`
- Added mesh commands:
  - `forge mesh catalog`
  - `forge mesh provision <node-id>`
  - `forge mesh report-auth <node-id> <profile-id> <ok|expired|missing>`

## Acceptance mapping

- New node provisioning in one command: `forge mesh provision <node-id>`.
- Mesh status now includes profile inventory + auth state per node and totals.

## Validation

```bash
cargo test -p forge-cli --lib mesh::tests:: -- --nocapture
cargo test -p forge-cli --lib tests::mesh_module_is_accessible -- --nocapture
cargo check -p forge-cli
```
