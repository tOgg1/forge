# Forge Quickstart

This guide walks through the loop-first workflow.

## Prerequisites

- Rust toolchain (workspace root `Cargo.toml`)
- Git
- A supported harness: `pi`, `opencode`, `codex`, or `claude`
- Optional (legacy parity tooling only): Go 1.25+ in `old/go/go.mod`

## Build

```bash
make build
```

Binaries are written to `./build/forge`.

## Initialize a repo

From the repo you want to run loops in:

```bash
./build/forge init
```

This creates `.forge/` scaffolding and a `PROMPT.md` if missing.

## Workflows (preview)

Workflow definitions live in `.forge/workflows/*.toml`.

```bash
./build/forge workflow ls
./build/forge workflow show <name>
./build/forge workflow validate <name>
```

## Configure profiles

Import aliases from common shell alias files (or `FORGE_ALIAS_FILE`, which can be a path list). When using defaults, Forge also detects installed harnesses on `PATH`:

```bash
./build/forge profile init
```

Or add one manually:

```bash
./build/forge profile add pi --name local
```

If you want separate Pi config directories per profile:

```bash
PI_CODING_AGENT_DIR="$HOME/.pi/agent-work" pi
```

Forge sets `PI_CODING_AGENT_DIR` from `profile.auth_home` automatically.

## Create a pool

```bash
./build/forge pool create default
./build/forge pool add default oc1
./build/forge pool set-default default
```

## Start loops

```bash
./build/forge up --count 1
./build/forge ps
```

## Delegated persistent agent flow (<10 min)

Use this when parent wants ad-hoc delegated work, outside loop iteration flow.

```bash
# 1) Spawn/reuse child and wait for completion state
./build/forge agent run "Audit current branch and list risky diffs" \
  --agent delegate-1 \
  --type codex \
  --wait idle

# 2) Reuse same child for a follow-up
./build/forge agent run "Create a 3-step patch plan" \
  --agent delegate-1 \
  --wait idle

# 3) Add correlation metadata for parent/task tracing
./build/forge agent run "Draft commit message options" \
  --agent delegate-1 \
  --task-id forge-ftz \
  --tag docs \
  --label epic=M10 \
  --wait idle
```

Harness mode guidance:

- Persistent delegation needs interactive harness mode.
- If spawn reports mode/capability mismatch, choose an interactive harness command/profile and retry.
- If agent is terminal (`stopped`/`error`), retry with `--revive`.

Migration note:

```bash
# old naming
./build/forge subagent run "Summarize open work"

# new naming
./build/forge agent run "Summarize open work"
```

Ownership modes for runner spawn:

- `--spawn-owner local` (default): always detached local spawn.
- `--spawn-owner auto`: prefer local `forged`; fallback to detached local spawn if daemon unavailable.
- `--spawn-owner daemon`: require daemon; fail if unavailable.

## Start loops with `rforged` daemon mode

Build side-by-side Rust binaries:

```bash
make build-rust-cli build-rust-daemon
```

Start daemon in terminal A (use explicit port to avoid conflicts):

```bash
./build/rforged --config ~/.config/forge/config.yaml --port 50061
```

Run loops in terminal B through that daemon:

```bash
export FORGE_DAEMON_TARGET=http://127.0.0.1:50061
./build/rforge migrate up
./build/rforge up \
  --name daemon-quickstart \
  --profile <profile> \
  --prompt-msg "daemon quickstart check" \
  --max-iterations 2 \
  --interval 1s \
  --spawn-owner daemon \
  --json
```

Health-check daemon ownership/liveness:

```bash
./build/rforge ps --json | jq '.[]? | {name,state,runs,runner_owner,runner_daemon_alive,runner_instance_id}'
```

Expected:
- `runner_owner` is `daemon`
- `runner_daemon_alive` is `true` while daemon is reachable

Recovery/stop path:

```bash
./build/rforge stop daemon-quickstart
./build/rforge resume daemon-quickstart --spawn-owner daemon
# stop daemon when done (or Ctrl-C if foreground):
kill -TERM <rforged-pid>
```

## Smart stop (optional)

Quantitative stop (command-based):

```bash
./build/forge up --name review \
  --quantitative-stop-cmd 'sv count --epic | rg -q "^0$"' \
  --quantitative-stop-exit-codes 0
```

Qualitative stop (judge iteration, agent prints `0` stop / `1` continue):

```bash
./build/forge up --name review \
  --qualitative-stop-every 5 \
  --qualitative-stop-prompt stop-judge
```

## Send messages and watch logs

```bash
./build/forge msg <loop-name> "Summarize the open tasks"
./build/forge logs <loop-name> -f
```

## Launch the TUI

```bash
./build/forge
# or
./build/forge tui
```

## Troubleshooting

See `docs/troubleshooting.md` for common fixes.
