# sv-cgj - forged node daemon + registration (2026-02-13)

## Scope shipped
- Added `forge node` command family in `crates/forge-cli/src/node.rs`:
  - `forge node ls`
  - `forge node add --name ... [--ssh ...] [--daemon-target ...] [--token ...] [--local]`
  - `forge node rm <id-or-name>`
  - `forge node status <id-or-name>` (forged `Ping` + `GetStatus` probe)
  - `forge node exec <id-or-name> --loop <loop-id> [--config ...] [--command-path ...]` (forged `StartLoopRunner`)
- Added sqlite-backed node registration persistence using existing `nodes` table (`metadata_json` stores daemon target + bearer token).
- Added daemon auth-header injection for node RPC requests when token is configured.
- Wired root CLI command dispatch/help/module accessibility:
  - `crates/forge-cli/src/lib.rs`

## Tests added
- `node::tests::add_and_list_registered_nodes`
- `node::tests::status_uses_rpc_and_reports_online_health`
- `node::tests::status_reports_offline_when_probe_fails`
- `node::tests::exec_starts_loop_via_daemon_rpc`
- `node::tests::exec_requires_loop_id_flag`
- `tests::node_module_is_accessible`

## Validation
```bash
cargo test -p forge-cli --lib node::tests:: -- --nocapture
cargo test -p forge-cli --lib tests::node_module_is_accessible -- --nocapture
cargo build -p forge-cli
```

## Notes
- Fixed compile drift in `crates/forge-cli/src/profile_catalog.rs` (mutable borrow lifetime around catalog write) encountered during validation.
