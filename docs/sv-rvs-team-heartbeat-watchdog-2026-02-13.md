# sv-rvs: Team heartbeat + watchdog

Date: 2026-02-13
Task: `sv-rvs`

## Scope delivered

- Added heartbeat/watchdog core module:
  - `TeamHeartbeatState` with per-team last heartbeat + restart counters
  - `heartbeat_tick(...)` for active heartbeat updates
  - `evaluate_watchdog(...)` for stale detection and degraded status transitions
  - crash-safe persistence helpers:
    - `persist_heartbeat_state(...)`
    - `restore_heartbeat_state(...)`
  - CLI/TUI visibility helper:
    - `render_team_heartbeat_rows(...)`
- Added DB-backed config loader:
  - `load_team_heartbeat_configs_from_db(...)`
  - derives interval + stale thresholds from team repository config

## Files

- `crates/forge-cli/src/team_heartbeat_watchdog.rs`
- `crates/forge-cli/src/lib.rs`

## Validation

```bash
cargo test -p forge-cli --lib team_heartbeat_watchdog::tests:: -- --nocapture
cargo build -p forge-cli
```
