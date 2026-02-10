# Rust Live Backend Wiring Update (2026-02-10)

## Wired in this pass

- `forge-cli ps` now uses SQLite (`forge_db`) instead of in-memory stubs.
- `forge-cli ps` now reads:
  - loop rows from `loops`,
  - run counts from `loop_runs`,
  - pending queue depth from `loop_queue_items`,
  - runner metadata from loop `metadata_json`.
- `forge-cli tui` now uses a process backend and launches `forge-tui` (no in-memory launch stub).
- `forge-tui` binary now renders live loop snapshot data from SQLite (refresh loop in interactive TTY).
- `fmail-tui` binary now renders live mailbox snapshot data from `.fmail` store.

## Validated

- `cargo test -p forge-cli ps::`
- `cargo test -p forge-cli tui::`
- `cargo test -p forge-tui -p fmail-tui`
- `cargo build --release -p forge-cli -p forge-tui -p fmail-tui`
- Runtime checks:
  - `rforge ps` now returns real loop rows from existing DB.
  - `rforge ps --json` returns real JSON rows.
  - `rforge-tui` now shows live loop snapshot output.
  - `rfmail-tui` now shows live mailbox snapshot output.

## Still not complete parity

- Many `forge-cli` commands still route through in-memory backends (`up`, `run`, `stop`, `scale`, `send`, `queue`, `resume`, `rm`, `logs`, `status`, `workflow`, etc.).
- `forge-tui` / `fmail-tui` are now live-data shells, but not feature-complete interactive parity with Go TUI yet.

## Immediate next wiring target

- Move loop lifecycle commands (`up`, `stop`, `run`, `scale`, `queue`, `send`) to SQLite + runtime process backends.
