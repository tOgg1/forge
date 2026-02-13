# sv-xgk: Remote registry query/update

## Scope delivered
- Added `forge node` command family with remote routing support via mesh registry.
- Added `forge node registry ls/show/update <node-id> ...` passthrough to remote `forge registry`.
- Added standalone `forge registry` command family with local store + list/show/update semantics.
- Root CLI wiring added for `node` and `registry` command families.

## Behavior
- `forge node registry` routes through mesh master when target is non-master.
- Offline/unknown node cases return explicit errors.
- Remote non-zero exits are surfaced to CLI with node context.
- Remote command construction safely shell-quotes each token.

## Validation
- `cargo test -p forge-cli --lib node::tests:: -- --nocapture`
- `cargo test -p forge-cli --lib registry::tests:: -- --nocapture`
- `cargo test -p forge-cli --lib tests::node_module_is_accessible -- --nocapture`
- `cargo test -p forge-cli --lib tests::registry_module_is_accessible -- --nocapture`
- `cargo check -p forge-cli`
