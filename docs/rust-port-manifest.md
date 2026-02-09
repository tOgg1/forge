# Rust Port Manifest (Include vs Ignore)

Date: 2026-02-09
Updated: 2026-02-09
Decision source: owner directives + current command wiring in `internal/cli`.
Scope lock: forge-kwk (non-legacy forge command manifest).

## Rules

- Port all non-legacy behavior.
- Ignore legacy interactive setup and legacy command groups wired via `addLegacyCommand(...)`.
- Single final switch; continuous parity checks required before switch.
- Every command explicitly classified: **port** or **drop**. No ambiguous status.

## Include: binaries

| Binary | Go source | Rust crate |
|--------|-----------|------------|
| `forge` | `cmd/forge` | `forge-cli` |
| `forged` | `cmd/forged` | `forge-daemon` |
| `forge-agent-runner` | `cmd/forge-agent-runner` | `forge-runner` |
| `fmail` | `cmd/fmail` | `fmail-cli` |
| `fmail-tui` | `cmd/fmail-tui` | `fmail-tui` |

## `forge` non-legacy command manifest

Status: **port** for all commands below.

### Loop lifecycle commands → `forge-cli` + `forge-loop`

| Command | Subcommands | Aliases | Go source | Notes |
|---------|-------------|---------|-----------|-------|
| `up` | — | — | `loop_up.go` | Start loop(s) for a repo |
| `stop` | — | — | `loop_control.go` | Stop loops after current iteration |
| `kill` | — | — | `loop_control.go` | Kill loops immediately |
| `resume` | — | — | `loop_resume.go` | Resume a stopped loop |
| `ps` | — | — | `loop_ps.go` | List loops |
| `rm` | — | — | `loop_rm.go` | Remove loop records |
| `clean` | — | — | `loop_clean.go` | Remove inactive loops |
| `scale` | — | — | `loop_scale.go` | Scale loops to target count |
| `run` | — | — | `loop_run_cmd.go` | Run a single loop iteration |
| `logs` | — | `log` | `loop_logs.go` | Tail loop logs |

### Queue/messaging commands → `forge-cli` + `forge-loop`

| Command | Subcommands | Aliases | Go source | Notes |
|---------|-------------|---------|-----------|-------|
| `msg` | — | — | `loop_msg.go` | Queue a message for loop(s) |
| `queue` | `list`, `clear`, `remove`, `move` | — | `loop_queue.go` | Manage loop queues |
| `inject` | — | — | `inject.go` | Inject message directly into agent (bypasses queue) |
| `send` | — | — | `send.go` | Queue a message for an agent |

### State/memory commands → `forge-cli` + `forge-core`

| Command | Subcommands | Aliases | Go source | Notes |
|---------|-------------|---------|-----------|-------|
| `mem` | `set`, `get`, `ls`, `rm` | — | `loop_mem.go` | Per-loop key/value memory |
| `work` | `set`, `clear`, `current`, `ls` | — | `loop_work.go` | Loop work context (task id + status) |

### Configuration/profile commands → `forge-cli` + `forge-core`

| Command | Subcommands | Aliases | Go source | Notes |
|---------|-------------|---------|-----------|-------|
| `config` | `init`, `path` | — | `config.go` | Global configuration |
| `init` | — | — | `init.go` | Initialize a repo for loops |
| `profile` | `list`, `add`, `edit`, `remove`, `init`, `doctor`, `cooldown` | — | `profile.go` | Harness profiles; `cooldown` has sub: `set`, `clear` |
| `pool` | `list`, `create`, `add`, `show`, `set-default` | — | `pool.go` | Profile pools |
| `prompt` | `list`, `add`, `edit`, `set-default` | — | `loop_prompt.go` | Loop prompts |
| `use` | — | `context` | `context.go` | Set current workspace/agent context |
| `context` | — | — | `context.go` | Show current context (alias for `use` with no args) |

### Template/sequence/workflow commands → `forge-cli` + `forge-core`

| Command | Subcommands | Aliases | Go source | Notes |
|---------|-------------|---------|-----------|-------|
| `template` | `list`, `show`, `add`, `edit`, `run`, `delete` | — | `template.go` | Message templates |
| `seq` | `list`, `show`, `add`, `edit`, `run`, `delete` | `sequence` | `sequence.go` | Sequences |
| `workflow` | `list`, `show`, `validate` | — | `workflow.go` | Workflows |
| `skills` | `bootstrap` | — | `skills.go` | Workspace skills |

### Operational commands → `forge-cli` + various crates

| Command | Subcommands | Aliases | Go source | Rust crate | Notes |
|---------|-------------|---------|-----------|------------|-------|
| `status` | — | — | `status.go` | `forge-cli` | Fleet status summary |
| `doctor` | — | — | `doctor.go` | `forge-cli` | Environment diagnostics |
| `explain` | — | — | `explain.go` | `forge-cli` | Explain agent/queue status |
| `audit` | — | — | `audit.go` | `forge-cli` + `forge-db` | View audit log |
| `export` | `status`, `events` | — | `export.go` | `forge-cli` + `forge-db` | Export Forge data |
| `wait` | — | — | `wait.go` | `forge-cli` + `forge-loop` | Wait for condition |
| `lock` | `claim`, `release`, `status`, `check` | — | `lock.go` | `forge-cli` + `forge-core` | Advisory file locks |
| `hook` | `on-event` | — | `hook.go` | `forge-cli` + `forge-core` | Event hooks |
| `mail` | `send`, `inbox`, `read`, `ack` | — | `mail.go` | `forge-cli` + `fmail-core` | In-forge mail access |
| `migrate` | `up`, `down`, `status`, `version` | — | `migrate.go` | `forge-cli` + `forge-db` | DB migrations |
| `completion` | — | — | `completion.go` | `forge-cli` | Shell completions |

### TUI command → `forge-cli` + `forge-tui`

| Command | Subcommands | Aliases | Go source | Notes |
|---------|-------------|---------|-----------|-------|
| `tui` | — | `ui` | `ui.go` | Launch loop TUI dashboard |

### Hidden/internal commands → `forge-cli` + `forge-loop`

| Command | Subcommands | Aliases | Go source | Notes |
|---------|-------------|---------|-----------|-------|
| `loop` (hidden) | `run` | — | `loop_internal.go` | Internal runner orchestration path |

### `forge` global flags (must be ported)

| Flag | Short | Type | Default | Notes |
|------|-------|------|---------|-------|
| `--chdir` | `-C` | string | — | Change working directory |
| `--config` | — | string | `$HOME/.config/forge/config.yaml` | Config file path |
| `--json` | — | bool | false | JSON output |
| `--jsonl` | — | bool | false | JSON Lines output |
| `--log-format` | — | string | — | Logging format override |
| `--log-level` | — | string | — | Logging level override |
| `--no-color` | — | bool | false | Disable colored output |
| `--no-progress` | — | bool | false | Disable progress output |
| `--non-interactive` | — | bool | false | No prompts, use defaults |
| `--quiet` | — | bool | false | Suppress non-essential output |
| `--robot-help` | — | bool | false | Machine-readable help |
| `--since` | — | string | — | Replay events since duration/timestamp |
| `--verbose` | `-v` | bool | false | Verbose output |
| `--watch` | — | bool | false | Watch for changes |
| `--yes` | `-y` | bool | false | Skip confirmation prompts |

## `fmail` command manifest

Status: **port** for all commands below.
Canonical matrix + snapshot parity checks:
`docs/rust-fmail-command-manifest.md`.

### `fmail` commands → `fmail-cli` + `fmail-core`

| Command | Subcommands | Aliases | Go source | Notes |
|---------|-------------|---------|-----------|-------|
| `send` | — | — | `internal/fmail/send.go` | Send message; flags: `--file`, `--reply-to`, `--priority`, `--tag`, `--json` |
| `log` | — | `logs` | `internal/fmail/log.go` | View recent messages; flags: `--limit`, `--since`, `--from`, `--follow`, `--json` |
| `messages` | — | — | `internal/fmail/log.go` | View all public messages; same flags as `log` |
| `watch` | — | — | `internal/fmail/watch.go` | Stream messages; flags: `--timeout`, `--count`, `--json` |
| `who` | — | — | `internal/fmail/who.go` | List known agents; flag: `--json` |
| `status` | — | — | `internal/fmail/status.go` | Show/set status; flag: `--clear` |
| `register` | — | — | `internal/fmail/register.go` | Request agent name; flag: `--json` |
| `topics` | — | `topic` | `internal/fmail/topics.go` | List topics; flag: `--json` |
| `gc` | — | — | `internal/fmail/gc.go` | Remove old messages; flags: `--days`, `--dry-run` |
| `init` | — | — | `internal/fmail/init.go` | Initialize mailbox; flag: `--project` |
| `completion` | — | — | (cobra builtin) | Shell completions |

### `fmail` global flags

| Flag | Short | Type | Default | Notes |
|------|-------|------|---------|-------|
| `--robot-help` | — | bool | false | Machine-readable help |
| `--version` | `-v` | — | — | Show version |

### `fmail-tui` flags → `fmail-tui` crate

| Flag | Short | Type | Default | Notes |
|------|-------|------|---------|-------|
| `--project` | — | string | — | fmail project ID override |
| `--root` | — | string | — | Project root containing `.fmail` |
| `--forged-addr` | — | string | — | forged endpoint |
| `--agent` | — | string | `$FMAIL_AGENT` | Sender identity |
| `--operator` | `-o` | bool | false | Start in operator console view |
| `--theme` | — | string | `default` | Theme: `default` or `high-contrast` |
| `--poll-interval` | — | duration | `2s` | Background refresh interval |

## Include: subsystem packages (Rust parity targets)

### Core/runtime/data → `forge-core`, `forge-db`, `forge-loop`

| Go package | Target Rust crate | Notes |
|------------|-------------------|-------|
| `internal/config` | `forge-core` | |
| `internal/db` (+ migrations) | `forge-db` | |
| `internal/models` | `forge-core` | |
| `internal/loop` | `forge-loop` | |
| `internal/harness` | `forge-loop` | |
| `internal/hooks` | `forge-core` | |
| `internal/logging` | `forge-core` | |
| `internal/events` | `forge-core` | |
| `internal/queue` | `forge-loop` | |
| `internal/templates` | `forge-core` | |
| `internal/sequences` | `forge-core` | |
| `internal/workflows` | `forge-core` | |
| `internal/skills` | `forge-core` | |
| `internal/procutil` | `forge-core` | |
| `internal/names` | `forge-core` | |

### Daemon/agent transport → `forge-daemon`, `forge-runner`

| Go package | Target Rust crate | Notes |
|------------|-------------------|-------|
| `internal/forged` | `forge-daemon` | |
| `internal/agent` | `forge-runner` | |
| `internal/agent/runner` | `forge-runner` | |
| `internal/node` | `forge-core` | Shared data model (non-legacy parts) |
| `internal/workspace` | `forge-core` | Shared data model (non-legacy parts) |
| `internal/tmux` | `forge-core` | |
| `internal/ssh` | `forge-core` | |
| `internal/scheduler` | `forge-loop` | |
| `internal/state` | `forge-core` | |
| `internal/account` | `forge-core` | Exclude `caam` subpackage |

### TUI/mail → `forge-tui`, `fmail-core`, `fmail-cli`, `fmail-tui`

| Go package | Target Rust crate | Notes |
|------------|-------------------|-------|
| `internal/looptui` | `forge-tui` | Replace with FrankenTUI impl |
| `internal/fmail` | `fmail-core` + `fmail-cli` | |
| `internal/fmailtui` | `fmail-tui` | + `data`, `layout`, `state`, `styles`, `threading` |
| `internal/agentmail` | `fmail-core` | |
| `internal/teammsg` | `fmail-core` | |

## Drop: legacy command groups

Status: **drop**. All wired via `addLegacyCommand(...)`. Not ported to Rust.

| Command | Subcommands | Go source | Notes |
|---------|-------------|-----------|-------|
| `accounts` | `add`, `list`, `cooldown` (`set`, `clear`), `rotate`, `import-caam` | `accounts.go` | Legacy account management |
| `agent` | `spawn`, `list`, `status`, `terminate`, `interrupt`, `pause`, `resume`, `send`, `restart`, `queue`, `approve` | `agent.go` | Legacy agent control |
| `attach` | — | `attach.go` | Legacy workspace attach |
| `node` | `list`, `add`, `remove`, `bootstrap`, `doctor`, `refresh`, `exec`, `tunnel`, `forward` | `node.go` | Legacy node management |
| `recipe` | `list`, `show`, `run` | `recipe.go` | Legacy recipes |
| `vault` | `init`, `backup`, `activate`, `list`, `delete`, `status`, `paths`, `clear`, `push`, `pull` | `vault.go` | Legacy vault management |
| `ws`/`workspace` | `create`, `import`, `list`, `status`, `beads-status`, `attach`, `remove`, `kill`, `refresh` | `workspace.go` | Legacy workspace management |

## Drop: legacy/dead packages

Status: **drop** (unless new explicit decision recorded here).

| Go package | Reason |
|------------|--------|
| `internal/tui` | Old dashboard stack; current `forge tui` uses `internal/looptui` |
| `internal/recipes` + `internal/recipes/builtin` | Legacy recipes |
| `internal/account/caam` | Legacy accounts import path |
| Legacy-only CLI files tied to dropped command groups | No non-legacy consumers |

## Boundary notes

- `send`, `inject`, `wait`, `status`, `export`, `doctor`, `completion` are **non-legacy and must be ported**, even though they touch older agent/workspace/node data paths.
- Legacy drop does not mean dropping shared data model pieces still needed by included commands.
- Any additional drop must be explicitly recorded in this manifest before implementation.
- `help` is auto-generated by clap in Rust; no manual port needed.
