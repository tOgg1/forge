# TUI/CLI runtime path contract: daemon-compatible aliases

Task: `forge-zxw`

## What shipped
- Added shared CLI runtime path resolver: `crates/forge-cli/src/runtime_paths.rs`.
- Unified all CLI `resolve_database_path()` callsites to the shared resolver.
- Unified CLI data-dir runtime lookups (`logs`, `run_exec`) to shared resolver.
- Updated `forge-tui` DB path resolver to honor daemon/global data-dir aliases.

## Env precedence
Database path:
1. `FORGE_DATABASE_PATH`
2. `FORGE_DB_PATH`
3. `<resolved_data_dir>/forge.db`

Data dir:
1. `FORGE_DATA_DIR`
2. `FORGE_GLOBAL_DATA_DIR`
3. `SWARM_GLOBAL_DATA_DIR`
4. `$HOME/.local/share/forge`
5. `.forge-data`

## Regression coverage
- `crates/forge-cli/src/runtime_paths.rs` unit tests for alias precedence.
- `crates/forge-cli/tests/root_command_test.rs`: `up_command_dispatches_with_global_data_dir_alias`.
- `crates/forge-tui/src/bin/forge-tui.rs` test for global data-dir alias DB resolution.

## Validation
- `cargo fmt --check` ✅
- `cargo clippy -p forge-tui --all-targets -- -D warnings` ✅
- `cargo test -p forge-cli` ✅
- `cargo test -p forge-tui` ⚠️ blocked by unrelated pre-existing compile drift in `crates/forge-tui/src/app.rs` (missing fields in `App` initializer).
