# Rust Rewrite: fmail Command Manifest

Scope lock task: `forge-n69`
Snapshot date: 2026-02-09

Snapshot sources:
- `docs/forge-mail/help/fmail-help.txt`
- `docs/forge-mail/help/fmail-tui-help.txt`

## Parity rules

- `fmail` with **no args** launches `fmail-tui` (current `cmd/fmail/main.go` behavior).
- Every `fmail` top-level command from `--help` is **port** (including builtins).
- Every `fmail-tui` CLI flag from `--help` is **port**.
- Help/version semantics stay compatible (`--help`, `--version`, cobra help command).

## `fmail` top-level command matrix

| Command | Status | Parity expectation |
|---|---|---|
| `completion` | port | Keep cobra shell completion generation behavior. |
| `gc` | port | Keep retention semantics and `--days`/`--dry-run` behavior. |
| `help` | port | Keep command help routing and exit semantics. |
| `init` | port | Keep mailbox initialization behavior and `--project` override. |
| `log` | port | Keep history read defaults + filtering semantics. |
| `messages` | port | Keep all-public-messages view semantics. |
| `register` | port | Keep unique-name negotiation semantics. |
| `send` | port | Keep topic/DM send behavior, priority/tags/reply metadata handling. |
| `status` | port | Keep read/set/clear status semantics. |
| `topics` | port | Keep topic activity listing + output shape. |
| `watch` | port | Keep streaming semantics (`--timeout`, `--count`). |
| `who` | port | Keep known-agent listing behavior. |

## `fmail` global flags

| Flag | Status | Parity expectation |
|---|---|---|
| `--robot-help` | port | Keep machine-readable help output format. |
| `--version` | port | Keep version-print behavior and exit code. |
| `--help` | port | Keep cobra help output path. |

## `fmail-tui` CLI flag matrix

| Flag | Status | Parity expectation |
|---|---|---|
| `--agent` | port | Keep compose identity override semantics. |
| `--forged-addr` | port | Keep forged endpoint override semantics. |
| `--operator` | port | Keep startup in operator console mode. |
| `--poll-interval` | port | Keep refresh cadence override semantics. |
| `--project` | port | Keep project-id override semantics. |
| `--root` | port | Keep `.fmail` root override semantics. |
| `--theme` | port | Keep accepted values and default. |
| `--version` | port | Keep version-print behavior and exit code. |
| `--help` | port | Keep help output path and exit semantics. |
