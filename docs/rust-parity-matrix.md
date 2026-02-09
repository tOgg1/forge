# Rust Rewrite Parity Matrix (Template)

Task: `forge-ham`
Version: `v0` (2026-02-09)

## Status scale

- `not-started`
- `in-progress`
- `parity-green`
- `blocked`

## Surface matrix

| Surface | Go source of truth | Rust target | Gate artifact | Status | Notes |
|---|---|---|---|---|---|
| Forge CLI help/flags | `cmd/forge`, `internal/cli` | `forge-cli` | parity snapshots + doccheck | not-started | |
| fmail CLI help/flags | `cmd/fmail`, `internal/fmail` | `fmail-cli` | `docs/forge-mail/help/*` + doccheck | in-progress | command manifest frozen |
| fmail-tui CLI flags | `cmd/fmail-tui`, `internal/fmailtui` | `fmail-tui` | `docs/forge-mail/help/*` + doccheck | in-progress | flag matrix frozen |
| DB migrations/schema | `internal/db/migrations` | `forge-db` | schema fingerprint tests | not-started | |
| Loop runtime semantics | `internal/loop`, `internal/queue` | `forge-loop` | characterization harness | not-started | |
| Daemon + runner protocol | `internal/forged`, `internal/agent/runner` | `forge-daemon` + `forge-runner` | proto compat tests | not-started | |
| Loop TUI workflows | `internal/looptui` | `forge-tui` | manual + scripted smoke | not-started | |
| fmail/fmail-tui workflows | `internal/fmail`, `internal/fmailtui` | `fmail-core` + `fmail-tui` | scripted + manual checklist | not-started | |

## Required evidence per gate

- CI required checks green (`parity`, `baseline-snapshot`, `rust-quality`, `rust-coverage`).
- Drift artifacts available for failures (`parity-diff`).
- Baseline snapshot artifact published (`rust-baseline-snapshot`).
- Rust coverage policy locked and enforced (`docs/rust-coverage-policy.md`).

## Change protocol

- Any parity status change requires linked evidence artifact/test.
- New surface rows must include explicit Go source and Rust target owner crate.
