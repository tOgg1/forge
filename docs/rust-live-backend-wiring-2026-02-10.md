# Rust Live Backend Wiring Update (2026-02-10)

## Wired in this pass

- `forge-cli ps` now uses SQLite (`forge_db`) instead of in-memory stubs.
- `forge-cli status` now uses SQLite (`forge_db`) instead of in-memory stubs.
- `forge-cli msg` now uses SQLite (`forge_db`) instead of in-memory stubs.
- `forge-cli ps` now reads:
  - loop rows from `loops`,
  - run counts from `loop_runs`,
  - pending queue depth from `loop_queue_items`,
  - runner metadata from loop `metadata_json`.
- `forge-cli status` now computes summary from:
  - loop rows from `loops`,
  - pending queue depth from `loop_queue_items`,
  - active profile cooldowns from `profiles.cooldown_until`,
  - runner liveness metadata from loop `metadata_json`.
- `forge-cli msg` now:
  - selects loops from `loops`,
  - supports repo/pool/profile/state/tag selectors against persisted rows,
  - enqueues queue items into `loop_queue_items`.
- `forge-cli inject` now uses SQLite + live tmux transport:
  - resolves agent from `agents` (ID/prefix + context fallback),
  - reads context from `~/.config/forge/context.yaml`,
  - sends direct pane input via `tmux send-keys` (bypasses queue).
- `forge-cli explain` now uses SQLite + context backend:
  - resolves agents from `agents` (ID/prefix + context/workspace fallback),
  - reads queue facts from `queue_items` with payload decoding parity,
  - reads account cooldown/profile facts from `accounts`.
- `forge-cli send` now uses SQLite + context backend:
  - resolves target agents from `agents` (ID/prefix + context/workspace auto-detect),
  - enqueues queue items into `queue_items` (tail/front/after + when-idle payloads),
  - preserves queue position semantics for human/json outputs.
- `forge-cli up` now uses SQLite (`forge_db`) backend:
  - creates loops in `loops` with real pool/profile ref resolution,
  - enqueues initial pause items into `loop_queue_items`,
  - marks loop `running` and persists runner metadata in `metadata_json`.
- `forge-cli tui` now uses a process backend and launches `forge-tui` (no in-memory launch stub).
- `forge-tui` binary now renders live loop snapshot data from SQLite (refresh loop in interactive TTY).
- `fmail-tui` binary now renders live mailbox snapshot data from `.fmail` store.

## Validated

- `cargo test -p forge-cli ps::`
- `cargo test -p forge-cli status::`
- `cargo test -p forge-cli msg::`
- `cargo test -p forge-cli inject::`
- `cargo test -p forge-cli explain::`
- `cargo test -p forge-cli send::`
- `cargo test -p forge-cli up::`
- `cargo test -p forge-cli tui::`
- `cargo test -p forge-tui -p fmail-tui`
- `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- `cargo build --release -p forge-cli -p forge-tui -p fmail-tui`
- Runtime checks:
  - `rforge ps` now returns real loop rows from existing DB.
  - `rforge ps --json` returns real JSON rows.
  - `rforge-tui` now shows live loop snapshot output.
  - `rfmail-tui` now shows live mailbox snapshot output.

## Still not complete parity

- Many `forge-cli` commands still route through in-memory backends (`run`, `scale`, `queue`, `mail`, `mem`, `workflow`, etc.).
- `forge-tui` / `fmail-tui` are now live-data shells, but not feature-complete interactive parity with Go TUI yet.

## Immediate next wiring target

- Continue loop lifecycle wiring (`run`, `scale`, `queue`) to SQLite + runtime process backends.
