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
- `forge delegation ...`
- `forge team ...`
- `forge task ...`
- `forge workflow ...`
- `forge job ...`
- `forge trigger ...`
- `forge registry ...`
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

### `forge tui`

Launch the loop TUI. Running plain `forge` (no subcommand) also opens TUI.

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

### `forge completion`

Generate shell completion scripts.

```bash
forge completion bash
forge completion zsh
forge completion fish
```

### `forge context`

Show current workspace/agent context (alias for `forge use --show`).

```bash
forge context
forge context --json
```

### `forge use`

Set or inspect current workspace/agent context.

```bash
forge use my-project
forge use ws_abc123
forge use --agent agent_xyz
forge use --show
forge use --clear
```

### `forge send`

Queue a message for an agent (safe queue-based dispatch).

```bash
forge send "Fix the lint errors"
forge send abc123 "Fix the lint errors"
forge send --all "Pause and commit your work"
```

### `forge inject`

Inject a message directly into an agent (bypasses queue).

```bash
forge inject abc123 "Stop and commit"
forge inject --force abc123 "Emergency stop"
forge inject abc123 --file prompt.txt
```

### `forge hook`

Register event hooks (command or webhook) that run on matching Forge events.

```bash
forge hook on-event --cmd "echo hook-fired"
forge hook on-event --url https://example.test/hook --type agent.state_changed
```

### `forge lock`

Manage advisory file locks for multi-agent coordination.

```bash
forge lock claim --agent agent-a --path crates/forge-cli/src/lib.rs --reason "editing"
forge lock status --path crates/forge-cli/src/lib.rs
forge lock release --agent agent-a --path crates/forge-cli/src/lib.rs
forge lock check --path crates/forge-cli/src/lib.rs
```

### `forge mail`

Send and read agent mailbox messages.

```bash
forge mail send --to agent-a1 --subject "Task handoff" --body "Please review PR #123"
forge mail inbox --agent agent-a1 --unread
forge mail read m-001 --agent agent-a1
forge mail ack m-001 --agent agent-a1
```

### `forge migrate`

Manage database schema migrations.

```bash
forge migrate status
forge migrate up
forge migrate down
forge migrate version
```

### `forge skills`

Manage workspace skills.

```bash
forge skills bootstrap
```

### `forge audit`

View audit log events with time/type/entity filters.

```bash
forge audit --since 1h
forge audit --type agent.state_changed --entity-type agent
forge audit --action message.dispatched --limit 200
```

### `forge doctor`

Run environment and capability diagnostics (deps/config/nodes/accounts).

```bash
forge doctor
forge doctor --json
```

### `forge explain`

Explain why an agent or queue item is in its current state.

```bash
forge explain
forge explain <agent-id>
forge explain <queue-item-id>
```

### `forge export`

Export Forge state for automation and reporting.

```bash
forge export events --json
forge export events --jsonl --type agent.spawned --since 1h
forge export status --json
forge export status --jsonl
```

### `forge status`

Show fleet status summary.

```bash
forge status
forge status --json
```

### `forge team`

Manage teams and team members.

```bash
forge team ls
forge team new ops --default-assignee agent-lead --heartbeat 300
forge team show ops
forge team member add ops agent-lead --role leader
forge team member ls ops
forge team member rm ops agent-lead
forge team rm ops
```

### `forge task`

Manage team task inbox lifecycle.

```bash
forge task send --team ops --type incident --title "database outage" --priority 5
forge task ls --team ops --status queued,assigned
forge task show <task-id>
forge task assign <task-id> --agent agent-a
forge task retry <task-id>
```

### `forge delegation`

Evaluate delegation rules against payloads and return routing decisions.

```bash
forge delegation route --payload '{"type":"incident","repo":"forge","priority":5}' --team ops
forge delegation explain --payload '{"type":"incident","repo":"forge","priority":5}' --team ops
forge delegation route --payload '{"type":"incident"}' --rules '{"default_agent":"agent-a","rules":[]}'
```

Notes:

- `--payload` is required.
- Use exactly one of `--rules` or `--team`.
- `route` returns final target decision; `explain` includes rule-check details.

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

Highlighting behavior, limits, customization:
- `docs/par-115-operator-highlighting-behavior-limits-customization.md`

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

## Workflow, job, and trigger commands

### `forge workflow`

Run and inspect workflows (DAG of steps).

```bash
forge workflow ls
forge workflow show <name>
forge workflow validate <name>
forge workflow run <name> --input repo=.
forge workflow run <name> --node <node-id>
forge workflow graph <name> --format dot
```

Concurrency:

- Global default from `~/.config/forge/config.yaml`: `scheduler.workflow_max_parallel`.
- Per-workflow override in workflow TOML: `max_parallel = <n>`.
- `forge workflow show <name>` prints resolved max parallel value and source.

### `forge job`

Manage job definitions and run history.

```bash
forge job ls
forge job show <name>
forge job run <name> [--trigger <source>] [--input key=value]
forge job runs <name> [--limit <n>]
forge job logs <name> [--limit <n>]
forge job cancel <run-id>
```

### `forge trigger`

Attach triggers to jobs (cron or webhook).

```bash
forge trigger ls
forge trigger add cron:0 2 * * * --job nightly-qa
forge trigger add webhook:/hooks/ship --job spec-to-ship
forge trigger rm <trigger-id>
```

### `forge registry`

Manage central registry entries for agents/prompts.

```bash
forge registry status [--repo <path>]
forge registry export [--repo <path>]
forge registry import [--repo <path>] [--prefer local|repo]
forge registry ls [all|agents|prompts]
forge registry show agent <name>
forge registry show prompt <name>
forge registry update agent <name> --harness <harness> --profile <profile> [--source <source>]
forge registry update prompt <name> --path <path> [--source <source>]
```

### `forge node`

Manage nodes in the mesh.

```bash
forge node ls
forge node exec <node> -- <command>
forge node registry ls <node> [agents|prompts]
forge node registry show <node> <agent|prompt> <name>
forge node registry update <node> agent <name> --harness <harness> --profile <profile> [--source <source>]
forge node registry update <node> prompt <name> --path <path> [--source <source>]
```

### `forge mesh`

Inspect or change mesh master.

```bash
forge mesh status
forge mesh catalog
forge mesh promote <node>
forge mesh demote <node>
forge mesh provision <node>
forge mesh report-auth <node> <profile-id> <ok|expired|missing>
```
