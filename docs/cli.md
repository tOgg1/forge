# Forge CLI Reference

This document describes the loop-centric CLI surface and the emerging workflow/job layout.

## Global usage

```bash
forge [flags] [command]
```

### Global flags

- `--config <path>`: Path to config file (default: `~/.config/forge/config.yaml`).
- `--json`: Emit JSON output (where supported).
- `--jsonl`: Emit JSON Lines output.
- `--quiet`: Suppress non-essential output.
- `-C, --chdir <path>`: Run in a specific repo directory.
- `--non-interactive`: Disable prompts and use defaults.
- `-v, --verbose`: Enable debug logging.
- `--log-level <level>`: Override logging level (`debug`, `info`, `warn`, `error`).
- `--log-format <format>`: Override logging format (`json`, `console`).

## CLI structure (proposed, incremental)

Keep backward compatibility. Existing top-level loop commands stay, but map to `forge loop ...`.

Canonical groups:

- `forge loop ...` (aliases: `forge up/ps/msg/stop/...`)
- `forge workflow ...`
- `forge job ...`
- `forge trigger ...`
- `forge node ...`
- `forge mesh ...`

Alias mapping:

- `forge up` -> `forge loop up`
- `forge ps` -> `forge loop ps`
- `forge msg` -> `forge loop msg`
- `forge logs` -> `forge loop logs`
- `forge stop` -> `forge loop stop`
- `forge kill` -> `forge loop kill`
- `forge resume` -> `forge loop resume`
- `forge rm` -> `forge loop rm`
- `forge clean` -> `forge loop clean`
- `forge scale` -> `forge loop scale`
- `forge queue` -> `forge loop queue`
- `forge run` -> `forge loop run`

## Core commands

### `forge agent` (persistent delegated agents)

Parent-oriented delegated workflow for non-loop tasks. Reuse one child across multiple tasks.

Key subcommands:

- `forge agent run [task-text] [--agent <id>] [--type <harness>] [--wait idle] [--revive|--revive-policy auto] [--approval-policy <mode>] [--account-id <id>] [--profile <name>]`
- `forge agent spawn [agent-id] --command <cmd>`
- `forge agent send <agent-id> <text> [--approval-policy <mode>] [--allow-risky]`
- `forge agent wait <agent-id> --until <state>`
- `forge agent ps`
- `forge agent show <agent-id>`
- `forge agent summary <agent-id>`
- `forge agent gc [--idle-timeout <sec>] [--max-age <sec>] [--dry-run]`
- `forge agent interrupt <agent-id> [--approval-policy <mode>] [--allow-risky]`
- `forge agent kill <agent-id> [--force]`

Real recipes:

```bash
# Recipe 1: one-command delegated run (spawn/reuse + send + optional wait)
forge agent run "Review service errors and summarize root cause" --agent triage-1 --type codex --wait idle

# Recipe 2: explicit spawn + iterative sends
forge agent spawn triage-2 --command codex
forge agent send triage-2 "Analyze failing tests in CI run 1284"
forge agent wait triage-2 --until idle
forge agent send triage-2 "Now propose minimal patch set"

# Recipe 3: terminal-state recovery + correlation metadata
forge agent run "Continue previous migration audit" \
  --agent migration-auditor \
  --revive-policy auto \
  --task-id forge-ftz \
  --tag persistent \
  --label epic=M10 \
  --wait idle
```

Policy notes:

- `agent run` propagates approval/account/profile context into spawned child environment (`FORGE_APPROVAL_POLICY`, `FORGE_ACCOUNT_ID`, `FORGE_PROFILE`).
- `agent send` and `agent interrupt` block risky actions under strict/default/plan policy unless `--allow-risky` is set.
- Persistent agent audit events redact common secret/token payloads.

Harness mode guidance:

- Persistent agents require interactive session capability.
- One-shot harness modes are not reusable for send/wait workflows.
- If you see a capability/mode mismatch error on spawn, switch harness command/profile to an interactive mode and retry.

Migration note (`subagent` -> `agent`):

```bash
# old
forge subagent run "Investigate flaky test"
forge subagent send reviewer-1 "Follow up with fix plan"

# new
forge agent run "Investigate flaky test"
forge agent send reviewer-1 "Follow up with fix plan"
```

### `forge` / `forge tui`

Launch the loop TUI.

```bash
forge
forge tui
```

TUI quick keys:

- `1/2/3/4`: switch tabs (`Overview`, `Logs`, `Runs`, `Multi Logs`)
- `]/[`: next/previous tab
- `t`: cycle color theme (`default`, `high-contrast`, `ocean`, `sunset`)
- `z`: zen mode (expand/collapse right pane)
- `j/k` or arrows: move selected loop
- `space`: pin/unpin selected loop for multi-log tab
- `m`: cycle multi-log layouts up to `4x4`
- `v`: cycle log source (`live`, `latest-run`, `selected-run`)
- `,` / `.`: previous/next run in logs/runs tabs
- `pgup` / `pgdown` / `home` / `end` / `u` / `d`: deep log scrolling in logs/runs/expanded views
- `l`: expanded log viewer
- `n`: new-loop wizard
- `/`: filter mode
- `S/K/D`: stop/kill/delete with confirmation

### `forge init`

Initialize `.forge/` scaffolding and optional `PROMPT.md`.

```bash
forge init
forge init --prompts-from ./prompts
forge init --no-create-prompt
```

### `forge config`

Manage global configuration at `~/.config/forge/config.yaml`.

```bash
forge config init          # Create default config with comments
forge config init --force  # Overwrite existing config
forge config path          # Print config file path
```

### `forge loop up` (alias: `forge up`)

Start loop(s) in the current repo.

```bash
forge up --count 1
forge up --name review-loop --prompt review
forge up --pool default --interval 30s --tags review
forge up --name review-loop --initial-wait 2m
forge up --max-iterations 10 --max-runtime 2h
forge up --spawn-owner local
forge up --spawn-owner daemon
forge up --quantitative-stop-cmd 'sv count --epic | rg -q "^0$"' --quantitative-stop-exit-codes 0
forge up --qualitative-stop-every 5 --qualitative-stop-prompt stop-judge
```

Smart stop (loop-level):

- Quantitative stop runs a shell command (repo workdir) and can match exit code/stdout/stderr. On match: stop or continue.
- Qualitative stop injects a specialized next iteration using the same agent. The agent must output `0` (stop) or `1` (continue).
See `docs/smart-stop.md`.

Loop runner ownership (`--spawn-owner`):

- `local` (default): detached local spawn.
- `auto`: try local `forged` daemon; if unavailable, warn and use detached local spawn.
- `daemon`: require daemon; fail if daemon unavailable.

### `forge loop ps` (alias: `forge ps`)

List loops.

```bash
forge ps
forge ps --state running
forge ps --pool default
forge ps --json
```

`forge ps` reconciles stale states before rendering:

- if loop state is `running` but runner PID is dead/missing and no daemon runner exists, loop is marked `stopped` with reason `stale_runner`.

JSON output includes runner diagnostics:

- `runner_owner`
- `runner_instance_id`
- `runner_pid_alive`
- `runner_daemon_alive`

### `forge loop logs` (alias: `forge logs`)

Tail loop logs.

```bash
forge logs review-loop
forge logs review-loop -f
forge logs --all
```

### `forge loop msg` (alias: `forge msg`)

Queue a message or override for a loop.

```bash
forge msg review-loop "Focus on the PRD changes"
forge msg review-loop --next-prompt ./prompts/review.md
forge msg --pool default --now "Interrupt and refocus"
forge msg review-loop --template stop-and-refocus --var reason=scope
forge msg review-loop --seq review-seq --var mode=fast
```

### `forge loop stop` / `forge loop kill` (aliases: `forge stop` / `forge kill`)

Stop or kill loops.

```bash
forge stop review-loop
forge kill review-loop
forge stop --pool default
```

### `forge loop resume` (alias: `forge resume`)

Resume a stopped or errored loop.

```bash
forge resume review-loop
forge resume review-loop --spawn-owner local
```

### `forge loop rm` (alias: `forge rm`)

Remove loop records (DB only). Logs and ledgers remain on disk. Use `--force` for selectors or running loops.

```bash
forge rm review-loop
forge rm --state stopped --force
forge rm --all --force
```

### `forge loop clean` (alias: `forge clean`)

Remove inactive loop records (stopped or errored). Logs and ledgers remain on disk.

```bash
forge clean
forge clean --repo .
forge clean --pool default
```

### `forge loop scale` (alias: `forge scale`)

Scale loops to a target count.

```bash
forge scale --count 3 --pool default
forge scale --count 3 --initial-wait 45s
forge scale --count 0 --kill
forge scale --count 2 --max-iterations 5 --max-runtime 1h
forge scale --count 3 --spawn-owner local
forge scale --count 3 --spawn-owner daemon
```

### `forge loop queue` (alias: `forge queue`)

Inspect or reorder the loop queue.

```bash
forge queue ls review-loop
forge queue clear review-loop
forge queue rm review-loop <item-id>
forge queue move review-loop <item-id> --to front
```

### `forge loop run` (alias: `forge run`)

Run a single iteration for a loop.

```bash
forge run review-loop
```

### `forge mem`

Persistent per-loop key/value memory (stored in Forge DB). Defaults to current loop via `FORGE_LOOP_ID`.

```bash
forge mem set blocked_on "waiting for agent-b"
forge mem get blocked_on
forge mem ls
forge mem rm blocked_on
```

### `forge work`

Persistent per-loop "what am I working on" pointer (stored in Forge DB). Task-tech-agnostic. Defaults to current loop via `FORGE_LOOP_ID`.

```bash
forge work set sv-1v3 --status blocked --detail "waiting for agent-b"
forge work current
forge work ls
forge work clear
```

## Prompt and template helpers

### `forge prompt`

Manage `.forge/prompts/`.

```bash
forge prompt ls
forge prompt add review ./prompts/review.md
forge prompt edit review
forge prompt set-default review
```

### `forge template`

Manage `.forge/templates/`.

```bash
forge template ls
forge template add review ./templates/review.md
forge template edit review
```

### `forge seq`

Manage `.forge/sequences/`.

```bash
forge seq ls
forge seq show review-seq
forge seq add review-seq ./sequences/review.seq.yaml
```

## Profiles and pools

### `forge profile`

Manage harness profiles.

```bash
forge profile ls
forge profile init
forge profile add pi --name local
forge profile edit local --max-concurrency 2
forge profile cooldown set local --until 30m
forge profile rm local
```

### `forge pool`

Manage profile pools.

```bash
forge pool ls
forge pool create default
forge pool add default oc1 oc2
forge pool set-default default
forge pool show default
```

## Workflow and job commands (planned)

### `forge workflow`

Run and inspect workflows (DAG of steps).

```bash
forge workflow ls
forge workflow show <name>
forge workflow validate <name>
forge workflow run <name> --input repo=.
forge workflow graph <name> --format dot
```

### `forge job`

Run higher-level jobs that can start workflows or dispatch work.

```bash
forge job ls
forge job show <name>
forge job run <name> --input repo=.
forge job logs <job-id>
forge job cancel <job-id>
```

### `forge trigger`

Attach triggers to jobs (cron or webhook).

```bash
forge trigger ls
forge trigger add cron:0 2 * * * --job nightly-qa
forge trigger add webhook:/hooks/ship --job spec-to-ship
forge trigger rm <trigger-id>
```

### `forge node`

Manage nodes in the mesh.

```bash
forge node ls
forge node add --ssh user@host --name <node>
forge node bootstrap --ssh root@host
forge node exec <node> -- <cmd>
forge node doctor <node>
```

### `forge mesh`

Inspect or change mesh master.

```bash
forge mesh status
forge mesh promote <node>
forge mesh demote <node>
forge mesh join <mesh-id>
forge mesh leave
```
