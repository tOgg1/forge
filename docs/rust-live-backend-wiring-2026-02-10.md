# Rust Live Backend Wiring Update (2026-02-10)

## Wired in this pass

- `forge-cli ps` now uses SQLite (`forge_db`) instead of in-memory stubs.
- `forge-cli status` now uses SQLite (`forge_db`) instead of in-memory stubs.
- `forge-cli msg` now uses SQLite (`forge_db`) instead of in-memory stubs.
- `forge-cli audit` now uses SQLite (`forge_db`) instead of in-memory stubs.
- `forge-cli mail` now uses a filesystem-backed `fmail-core` bridge instead of in-memory stubs.
- `forge-cli mem` now uses SQLite (`forge_db`) instead of in-memory stubs.
- `forge-cli workflow` now uses filesystem-backed workflow definitions from `.forge/workflows` instead of in-memory stubs.
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
- `forge-cli audit` now:
  - reads event rows from `events`,
  - applies persisted cursor/since/until/entity/event-type filters,
  - preserves table/json/jsonl output behavior from command parser.
- `forge-cli mail` now:
  - sends messages via `fmail_core::store::Store::save_message`,
  - reads inbox/read views from DM files under `.fmail`,
  - persists read/ack status in local metadata files under `.fmail`.
- `forge-cli mem` now:
  - resolves loop references from `loops`,
  - persists loop memory via `loop_kv`,
  - supports set/get/ls/rm against real DB rows.
- `forge-cli workflow` now:
  - lists workflow TOML files from `<repo>/.forge/workflows`,
  - resolves `show` / `validate` by workflow name against real files,
  - preserves validation diagnostics and JSON/human output behavior.
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
- `forge-cli kill` now uses SQLite (`forge_db`) backend:
  - resolves loops from `loops`,
  - enqueues `kill_now` into `loop_queue_items`,
  - persists loop `stopped` state and best-effort process signal.
- hidden `forge loop run` now uses SQLite (`forge_db`) backend via `run::SqliteRunBackend`:
  - resolves loop refs from `loops`,
  - records loop run rows in `loop_runs`,
  - updates `loops.last_run_at` and `last_exit_code`.
- `forge-cli export` now uses SQLite (`forge_db`) backend:
  - `export status` reads `nodes`, `workspaces`, `agents`, `queue_items`, `alerts`,
  - `export events` reads `events` via `EventRepository` cursor pagination,
  - payload/metadata now come from persisted DB event rows.
- `forge-cli tui` now uses a process backend and launches `forge-tui` (no in-memory launch stub).
- `forge-tui` binary now renders live loop snapshot data from SQLite (refresh loop in interactive TTY).
- `fmail-tui` binary now renders live mailbox snapshot data from `.fmail` store.
- `forge-cli` root runtime dispatch now has no in-memory DB backends left for core loop flow commands.

## Validated

- `cargo test -p forge-cli ps::`
- `cargo test -p forge-cli status::`
- `cargo test -p forge-cli msg::`
- `cargo test -p forge-cli audit::`
- `cargo test -p forge-cli mail::`
- `cargo test -p forge-cli mem::tests::`
- `cargo test -p forge-cli --test mem_command_test`
- `cargo test -p forge-cli --test workflow_filesystem_backend_test`
- `cargo test -p forge-cli --test workflow_command_test`
- `cargo test -p forge-cli inject::`
- `cargo test -p forge-cli explain::`
- `cargo test -p forge-cli send::`
- `cargo test -p forge-cli up::`
- `cargo test -p forge-cli --test root_command_test`
- `cargo test -p forge-cli --test export_command_test`
- `cargo test -p forge-cli tui::`
- `cargo test -p forge-tui -p fmail-tui`
- `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- `cargo build --release -p forge-cli -p forge-tui -p fmail-tui`
- `EDITOR=true VISUAL=true cargo test -p forge-cli`
- Runtime checks:
  - `rforge ps` now returns real loop rows from existing DB.
  - `rforge ps --json` returns real JSON rows.
  - `rforge loop run <id>` now increments persisted loop runs in DB.
  - `rforge kill <id>` now updates persisted loop state in DB.
  - `rforge export events --json` now returns persisted DB events.
  - `rforge-tui` now shows live loop snapshot output.
  - `rfmail-tui` now shows live mailbox snapshot output.

## Still not complete parity

- `forge-tui` / `fmail-tui` are now live-data shells, but not feature-complete interactive parity with Go TUI yet.
- In-memory backends remain in code for unit-test doubles, not runtime dispatch.

## Immediate next wiring target

- TUI and CLI parity polish: id-prefix matching UX, colored `ps`, richer `logs` highlighting, Frankentui feature parity.
