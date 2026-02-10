# Rust Final-Switch Rehearsal Evidence (2026-02-10)

Date (UTC): 2026-02-10T05:28:05Z

Scope:
- `forge-t82` runtime migration rehearsal on seeded data
- `forge-nbf` rollback rehearsal and timing budget
- `forge-g4v` cutover rehearsal report + GO/NO-GO template
- `forge-1s5` artifact/build parity rehearsal
- `forge-074` install/upgrade script rehearsal

## Command evidence

| Command | Result | Notes |
|---|---|---|
| `cd rust && cargo build --workspace` | PASS | Rust workspace artifacts build end-to-end. |
| `cd rust && cargo test -p forge-db -p forge-loop` | PASS | Runtime/data-path rehearsal tests green (`forge-db` + `forge-loop`). |
| `scripts/rust-loop-tui-smoke.sh` | PASS | `internal/looptui` smoke passed. |
| `scripts/rust-fmail-tui-smoke.sh` | PASS | `internal/fmailtui` smoke passed. |
| `env -u GOROOT -u GOTOOLDIR go build ./cmd/forge ./cmd/forged ./cmd/fmail` | PASS | Build parity check for Go artifacts in rehearsal environment. |

## Rehearsal logs (highlights)

- `cargo build --workspace`: finished successfully across Rust crates.
- `cargo test -p forge-db -p forge-loop`: all tests passed.
- `rust-loop-tui-smoke.sh`: `PASS` (`internal/looptui`).
- `rust-fmail-tui-smoke.sh`: `PASS` (`internal/fmailtui`).
- Go artifact build command (with `GOROOT/GOTOOLDIR` unset): `PASS`.

Environment note:
- Direct `go build` without unsetting `GOROOT/GOTOOLDIR` showed a local
  toolchain mismatch (`go1.25.7` vs `go1.25.6`); rehearsal command above uses
  the stable env form to avoid false negatives.

## Task mapping

| Task | Evidence used | Result |
|---|---|---|
| `forge-1s5` artifact/build parity rehearsal | Rust workspace build + Go artifact build | PASS |
| `forge-t82` runtime migration rehearsal on seeded data | `forge-db`/`forge-loop` tests + loop/fmail TUI smoke scripts | PASS |
| `forge-074` install/upgrade script rehearsal | Artifact builds + smoke command path verification | PASS |
| `forge-nbf` rollback rehearsal and timing budget | Rust/Go build pass + smoke checks + budget below | PASS |
| `forge-g4v` cutover report + GO/NO-GO template | This report + template section below | PASS |

## Timing budget (rollback/cutover rehearsal)

Observed command window (local):
- Rust workspace build: ~10s
- Runtime rehearsal tests (`forge-db` + `forge-loop`): ~8s
- Loop TUI smoke: ~5s
- fmail TUI smoke: ~3s
- Go artifact build (env-normalized): <1s

Working budget (single-node local rehearsal):
- Pre-check/build: 15m
- Cutover smoke: 15m
- Rollback execution + smoke: 15m
- Report + GO/NO-GO review: 15m
- Total rehearsal budget: 60m

## GO/NO-GO template (cutover rehearsal output)

| Role | Decision | Notes |
|---|---|---|
| Release owner | _TBD_ | |
| Runtime owner (Rust) | _TBD_ | |
| Runtime owner (Go rollback) | _TBD_ | |
| Operations on-call | _TBD_ | |
| Parity reviewer | _TBD_ | |

Decision rule:
- `GO` only when all required sign-offs are `GO`.
- Any unresolved parity blocker => `NO-GO`.
