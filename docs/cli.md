# Forge CLI Reference

This document describes the current CLI surface. Commands that are not wired
up yet are listed under Planned.

## Global usage

```bash
forge [flags] [command]
```

### Global flags

- `--config <path>`: Path to config file (default: `~/.config/forge/config.yaml`).
- `--json`: Emit JSON output (where supported).
- `--jsonl`: Emit JSON Lines output (streaming friendly).
- `--watch`: Stream updates until interrupted (reserved for future commands).
- `--no-color`: Disable colored output in human mode.
- `-v, --verbose`: Enable verbose output (forces log level `debug`).
- `--log-level <level>`: Override logging level (`debug`, `info`, `warn`, `error`).
- `--log-format <format>`: Override logging format (`json`, `console`).

## Commands

### `forge`

Launches the TUI. Current builds print a placeholder message.

```bash
forge
```

### `forge migrate`

Manage database migrations.

```bash
forge migrate [command]
```

#### `forge migrate up`

```bash
forge migrate up
forge migrate up --to 1
```

#### `forge migrate down`

```bash
forge migrate down
forge migrate down --steps 2
```

#### `forge migrate status`

```bash
forge migrate status
forge migrate status --json
```

#### `forge migrate version`

```bash
forge migrate version
forge migrate version --json
```

### `forge node`

Manage nodes.

```bash
forge node list
forge node add --name local --local
forge node add --name prod --ssh ubuntu@host --key ~/.ssh/id_rsa
forge node remove <name-or-id> --force
forge node doctor <name-or-id>
forge node refresh [name-or-id]
forge node exec <name-or-id> -- uname -a
forge node forward <name-or-id> --local-port 8080 --remote 127.0.0.1:3000
forge node tunnel <name-or-id>
```

Notes:
- `forge node bootstrap` exists but only reports missing deps today.
- Use `--no-test` on `node add` to skip connection test.
- `node add` supports per-node SSH preferences (backend, timeout, proxy jump, control master) via flags.
- `node forward` creates a local SSH tunnel for remote services (binds to `127.0.0.1` by default).
- `node tunnel` is a shortcut for forwarding forged (defaults to `127.0.0.1:50051`).

Secure access tip:
Use `forge node forward` instead of opening remote ports. Keep remote services bound to
`127.0.0.1` and expose them locally via an SSH tunnel when needed.

### `forge ws`

Manage workspaces.

```bash
forge ws create --path /path/to/repo --node local
forge ws import --session repo-session --node local
forge ws list
forge ws status <id-or-name>
forge ws beads-status <id-or-name>
forge ws attach <id-or-name>
forge ws remove <id-or-name> --destroy
forge ws refresh [id-or-name]
```

Notes:
- `ws remove --destroy` kills the tmux session after removing the workspace.
- Use `ws create --no-tmux` to track an existing session without creating one.
- If multiple repo roots are detected during `ws import`, pass `--repo-path` to select the correct root.
- New workspaces create a tmux session with window 0/pane 0 reserved for human interaction; agents are spawned in the `agents` window.

### `forge agent`

Manage agents.

```bash
forge agent spawn --workspace <ws> --type opencode --count 1
forge agent list --workspace <ws>
forge agent status <agent-id>
forge agent send <agent-id> "message"
forge agent send <agent-id> --file prompt.txt
forge agent send <agent-id> --stdin
forge agent send <agent-id> --editor
forge agent queue <agent-id> --file prompts.txt
forge agent pause <agent-id> --duration 5m
forge agent resume <agent-id>
forge agent interrupt <agent-id>
forge agent restart <agent-id>
forge agent terminate <agent-id>
```

Notes:
- `agent send` is deprecated and now queues messages (alias for `forge send`).
- Use `forge send --immediate` or `forge inject` for immediate dispatch.

### `forge mail`

Send and read Agent Mail messages.

```bash
forge mail send --to agent-a1 --subject "Task handoff" --body "Please review PR #123"
forge mail inbox --agent agent-a1
forge mail read m-001 --agent agent-a1
forge mail ack m-001 --agent agent-a1
```

Notes:
- Uses Agent Mail MCP when configured; otherwise stores messages in `~/.config/forge/mail.db`.
- Configure MCP with `FORGE_AGENT_MAIL_URL`, `FORGE_AGENT_MAIL_PROJECT`, and `FORGE_AGENT_MAIL_AGENT` (legacy `SWARM_*` also works).

### `forge send`

Queue messages for agents (safe, queue-first).

```bash
forge send <agent-id> "message"
forge send "message"                     # Uses agent context
forge send --all "message"               # Sends to all agents in workspace
forge send --priority high <agent-id> "message"
forge send --front <agent-id> "message"
forge send --after <queue-item-id> <agent-id> "message"
forge send --when-idle <agent-id> "message"
forge send --immediate <agent-id> "message"   # Deprecated; bypasses queue
```

Notes:
- `--immediate` is deprecated; prefer `forge inject` when you need direct tmux injection.

### `forge queue`

Inspect queued messages and dispatch status.

```bash
forge queue ls
forge queue ls --agent <agent-id>
forge queue ls --status pending
forge queue ls --all
```

Notes:
- Uses workspace context by default; pass `--agent` to scope to one agent.
- `--status blocked` shows pending items that are blocked by dependencies or agent state.

### `forge accounts`

Manage provider accounts and cooldowns.

```bash
forge accounts add
forge accounts list
forge accounts cooldown list
forge accounts cooldown set <account> --until 30m
forge accounts cooldown clear <account>
forge accounts rotate <agent-id> --reason manual
```

`forge accounts add` prompts for provider, profile, and credential source. If you enter a secret directly, Forge stores it in `~/.local/share/forge/credentials` and records a `file:` reference.

### `forge export`

Export Forge status.

```bash
forge export status --json
```

Human mode prints a summary; JSON/JSONL return full payloads.

### `forge export events`

Export the event log with optional filters.

```bash
forge export events --since 1h --jsonl
forge export events --since 1h --until now --jsonl
forge export events --type agent.state_changed,node.online --jsonl
forge export events --agent <agent-id> --jsonl
forge export events --watch --jsonl
```

### `forge audit`

View the audit log with filters for time, entity, and action.

```bash
forge audit --since 1h
forge audit --type agent.state_changed --entity-type agent
forge audit --action message.dispatched --limit 200
forge audit --json
```

## Planned commands

These are defined in the product spec but not wired up yet.

### `forge agent approve`

```bash
forge agent approve <agent-id> [--all]
```

### `forge ws kill` / `forge ws unmanage`

```bash
forge ws kill <id-or-name>
forge ws unmanage <id-or-name>
```
