# forge-vkn: forge-cli team heartbeat unwrap-used slice (2026-02-13)

## Scope
Remove clippy `unwrap_used` callsites in `crates/forge-cli/src/team_heartbeat_watchdog.rs` tests.

## Changes
Converted 7 unwrap callsites to explicit handling:

- map lookup for seeded heartbeat entry
- state persist/restore calls
- db open/migrate/create team/load configs in repository-backed test

Also formatted touched file.

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-cli/src/team_heartbeat_watchdog.rs
cargo clippy -p forge-cli --all-targets -- -D warnings
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli --lib heartbeat_tick_updates_timestamp
cargo test -p forge-cli --lib config_loads_intervals_from_team_repository
```

Results:

- full clippy still fails elsewhere, but no remaining `team_heartbeat_watchdog.rs` diagnostics
- clippy slice with unwrap/expect allowed passed
- both focused heartbeat tests passed
