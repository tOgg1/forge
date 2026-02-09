# Rust Port Manifest (Include vs Ignore)

Date: 2026-02-09
Decision source: owner directives + current command wiring in `internal/cli`.

## Rules

- Port all non-legacy behavior.
- Ignore legacy interactive setup and legacy command groups wired via `addLegacyCommand(...)`.
- Single final switch; continuous parity checks required before switch.

## Include: binaries

- `cmd/forge`
- `cmd/forged`
- `cmd/forge-agent-runner`
- `cmd/fmail`
- `cmd/fmail-tui`

## Include: `forge` non-legacy top-level commands

- `audit`
- `clean`
- `completion`
- `config`
- `context`
- `doctor`
- `explain`
- `export`
- `hook`
- `init`
- `inject`
- `kill`
- `lock`
- `logs`
- `mail`
- `mem`
- `migrate`
- `msg`
- `pool`
- `profile`
- `prompt`
- `ps`
- `queue`
- `resume`
- `rm`
- `run`
- `scale`
- `send`
- `seq`
- `skills`
- `status`
- `stop`
- `template`
- `tui`
- `up`
- `use`
- `wait`
- `work`
- `workflow`

Internal/non-user but required for runtime:

- hidden `forge loop run <loop-id>` (runner/internal orchestration path)

## Include: `fmail` commands

- `completion`
- `gc`
- `init`
- `log`/`logs`
- `messages`
- `register`
- `send`
- `status`
- `topics`/`topic`
- `watch`
- `who`

## Include: subsystem packages (Rust parity targets)

Core/runtime/data:

- `internal/config`
- `internal/db` (+ migrations)
- `internal/models`
- `internal/loop`
- `internal/harness`
- `internal/hooks`
- `internal/logging`
- `internal/events`
- `internal/queue`
- `internal/templates`
- `internal/sequences`
- `internal/workflows`
- `internal/skills`
- `internal/procutil`
- `internal/names`

Daemon/agent transport path (still needed by non-legacy commands and binaries):

- `internal/forged`
- `internal/agent`
- `internal/agent/runner`
- `internal/node`
- `internal/workspace`
- `internal/tmux`
- `internal/ssh`
- `internal/scheduler`
- `internal/state`
- `internal/account` (exclude `caam` subpackage; see ignore list)

TUI/mail path:

- `internal/looptui` (replace with Rust+FrankenTUI implementation)
- `internal/fmail`
- `internal/fmailtui` (+ `data`, `layout`, `state`, `styles`, `threading`)
- `internal/agentmail`
- `internal/teammsg`

## Ignore: legacy command groups

Legacy groups are currently wired with `addLegacyCommand(...)` and can be dropped in Rust:

- `accounts`
- `agent`
- `attach`
- `node`
- `recipe`
- `vault`
- `ws`/`workspace`

## Ignore: legacy/dead package areas (unless new explicit decision)

- `internal/tui` (old dashboard stack; current `forge tui` uses `internal/looptui`)
- `internal/recipes` and `internal/recipes/builtin`
- `internal/account/caam` (legacy accounts import path)
- legacy-only CLI files tied only to ignored command groups

## Important boundary notes

- `send`, `inject`, `wait`, `status`, `export`, `doctor`, `completion` are non-legacy and must be ported, even though they touch older agent/workspace/node data paths.
- Legacy drop does not mean dropping shared data model pieces still needed by included commands.
- Any additional drop must be explicitly recorded in this manifest before implementation.
