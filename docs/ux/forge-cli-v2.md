# Forge CLI v2 Specification (UX/DX First)

This document specifies the **new CLI surface** for Forge.
It focuses on:
- **Discoverability** (help output teaches workflows)
- **Safe defaults** (queue-first, no accidental injection)
- **Speed** (context-aware defaults, fuzzy/interactive selection)
- **Automation** (stable `--json/--jsonl` outputs)

> NOTE: This is a **spec**. Some commands may wrap existing implementations (`ws`, `agent`, `accounts`, etc.) until the codebase is refactored.

---

## Design principles

### 1) “Happy path” is 3 commands
Most users should be able to:
1. `forge up` (bootstrap workspace + tmux + agents)
2. `forge send` (queue instructions)
3. `forge ui` (watch and intervene)

### 2) Queue-first UX
- `forge send` **enqueues** by default (safe)
- Scheduler dispatches when agent becomes eligible (idle, not paused, account not cooling down)
- “Direct injection” exists but is **explicit** (`forge inject ...`)

### 3) Context-aware defaults
If you run commands inside a git repo:
- Forge should auto-select the matching workspace (or offer an interactive pick).
- If multiple workspaces match, prompt unless `--non-interactive`.

### 4) Everything scriptable
Every command supports:
- `--json` for machine-readable output
- `--jsonl` for streaming
- Stable exit codes and error envelopes (per `docs/ux/cli-style.md`)

---

## Command tree (exact)

```text
forge [flags]                       # default: launch TUI (same as `forge ui`)
  ui                                # launch TUI explicitly
  up [path]                         # create/open workspace for repo, ensure tmux, spawn agents
  ls                                # list workspaces (alias: `forge ws list`)
  ps                                # list agents (alias: `forge agent list`)
  use <workspace|agent>             # set default context for subsequent commands (kubectl-like)

  send [message]                    # enqueue message(s) to agent(s) / workspace
  inject [message]                  # immediate tmux injection (dangerous; explicit)
  queue                             # manage per-agent queues and scheduling
    ls [--agent ...]                # show queue items
    add                             # add queue item(s): message/pause/conditional
    edit                            # interactive queue editor (TUI-style, but CLI)
    clear                           # clear queue
    bump                            # move item to front/top
    rm                              # remove queue items
    run                             # request immediate dispatch (nudge scheduler)

  template                          # stored message templates
    ls
    show <name>
    add <name>
    edit <name>
    rm <name>
    run <name>                      # enqueue template-expanded message(s)

  seq                               # stored multi-step sequences (macros)
    ls
    show <name>
    add <name>
    edit <name>
    rm <name>
    run <name>                      # enqueue a whole sequence (messages + pauses + conditions)

  ws                                # workspace operations
    create [path]
    import --session <tmux> --node <node>
    attach <id-or-name>
    list
    status <id-or-name>
    refresh [id-or-name]
    remove <id-or-name>
    kill <id-or-name>

  agent                             # agent lifecycle and control
    spawn --workspace <ws> [flags]
    list
    status <agent-id>
    pause <agent-id> [--duration ...]
    resume <agent-id>
    stop <agent-id>                 # (rename of terminate; keep alias)
    restart <agent-id>
    send <agent-id> [message]       # low-level direct message; prefer `forge send`
    queue <agent-id> [...]          # low-level queue; prefer `forge queue`
    approve <agent-id>              # approvals / continue gates

  mail                              # Forge Mail (human CLI)
    inbox
    send
    read
    ack
    search

  lock                              # advisory file locks (human CLI)
    claim
    release
    status

  node                              # node mesh mgmt
    add
    list
    status <name-or-id>
    refresh [name-or-id]
    bootstrap <name-or-id>          # automated provisioning

  accounts                          # account profiles & usage
    list
    add
    set
    rotate <agent-id>
    cooldown
    clear

  status                            # system-wide summary (human default)
  events                            # export events (one-shot)
  watch                             # stream events (JSONL-first; for dashboards)
  export                            # export status/events in bulk

  init                              # first-run setup / config / migrations
  migrate                           # DB migrations
  doctor                            # environment + dependency check
  completion [bash|zsh|fish]
```

### Aliases and “fast path” commands

- `forge` = `forge ui`
- `forge ls` = `forge ws list`
- `forge ps` = `forge agent list`
- `forge stop` = `forge agent terminate` (keep old spelling as alias for backwards-compat)
- `forge seq` = `forge sequence` (optional long name; short is default)

---

## Global flags (proposed)

These are global so you can do: `forge --workspace myrepo send "continue"`.

- `--workspace, -w <name|id>`: default workspace context
- `--agent, -a <id|name>`: default agent target
- `--node <name|id>`: default node filter
- `--profile, -p <profile>`: default account profile
- `--json | --jsonl`: output mode
- `--watch`: for commands that can stream
- `--non-interactive`: disable prompts (also `FORGE_NON_INTERACTIVE=1`, legacy `SWARM_NON_INTERACTIVE=1`)
- `--no-color`: disable ANSI output (also `NO_COLOR=1`)
- `--config <path>`: explicit config file
- `--db <path>`: explicit local DB path (advanced)

---

## Help text (copy-pasteable)

Below are suggested `Short`, `Long`, and `Example` strings to paste into Cobra commands.

### `forge` (root)

**Short:**  
Launch the Forge TUI or run CLI subcommands.

**Long:**  
Forge manages agent workspaces (repo + node + tmux) and controls OpenCode agents.
Run without a subcommand to open the TUI.

**Examples:**
- `forge`  
- `forge up`  
- `forge send "run tests and fix failures"`  
- `forge ps --workspace myrepo`  
- `forge queue ls --agent A1 --json`

### `forge up`

**Short:**  
Create or open a workspace for a repo, ensure tmux, and optionally spawn agents.

**Examples:**
- `forge up` (uses current repo; prompts if ambiguous)
- `forge up . --agents 6 --type opencode`
- `forge up /srv/repos/api --node buildbox-1`
- `forge up --attach` (open tmux/session after bringing it up)

**Key flags (proposed):**
- `--node <node>` (default: local)
- `--name <workspace-name>` (default: derived from repo)
- `--session <tmux-session>` (default: derived)
- `--agents <n>` (default: 1)
- `--type <opencode|...>` (default: opencode)
- `--profile <profile>` (default: auto/next available)
- `--recipe <name>` (optional preset: agents + templates + sequences)
- `--attach` (attach tmux after provisioning)

### `forge send`

**Short:**  
Queue a message for one or more agents (safe default).

**Long:**  
Queues message(s) to the target agent(s). If the scheduler is running and an agent
is eligible (idle, not paused, not cooling down), the message will dispatch
automatically. Use `forge inject` only when you explicitly need immediate tmux injection.

**Examples:**
- `forge send "continue"` (targets current context)
- `forge send -a A1 "fix the failing test"`
- `forge send -w myrepo --all "pull latest, run tests, report status"`
- `forge send --template pr-review --to agent:A1` (template expansion)
- `forge send --pause 10m --then "resume and continue"` (sequence sugar)

**Key flags (proposed):**
- `--to <selector>` repeatable (e.g. `--to agent:A1 --to agent:A2`)
- `--all` (all agents in workspace)
- `--front` (insert at front of queue)
- `--pause <duration>` (insert pause after message)
- `--when-idle` (insert conditional gate)
- `--after-cooldown <duration>` (insert conditional gate)

### `forge template`

**Short:**  
Manage reusable message templates.

**Examples:**
- `forge template add implement-feature`
- `forge template run implement-feature -a A1 --var branch=feat/x`
- `forge template ls`

**Template format (proposed):**
- stored as YAML or Markdown in:
  - project: `.forge/templates/*.md|*.yaml`
  - global: `~/.config/forge/templates/...`
- supports simple variable substitution: `${var}`

### `forge seq`

**Short:**  
Manage multi-step sequences (macros) of queue items.

**Examples:**
- `forge seq add bugfix-loop`
- `forge seq run bugfix-loop -w api --all`
- `forge seq show bugfix-loop`

**Sequence format (proposed):**
A YAML list of steps:
- message
- pause
- conditional gates (when idle / after cooldown / after previous)

### `forge doctor`

**Short:**  
Check environment dependencies and report what to fix.

**Examples:**
- `forge doctor`
- `forge doctor --json`

Checks:
- tmux installed + usable
- ssh config sanity (if nodes configured)
- OpenCode available in PATH
- DB writable
- required directories exist

---

## UX/DX “missing pieces” to add next

These are the top things missing today to make the CLI *feel* incredible:

1. **Context (`forge use`)**
   - store default workspace/agent in `~/.config/forge/context.json`
   - all commands fall back to it (and to current repo) before prompting

2. **Selectors**
   - allow `--to ws:<name>` and `--to agent:<name>`
   - add `--filter state=idle` for bulk ops

3. **Template + Sequence engine**
   - make it trivial to “spin up 8 agents and send a standard boot sequence”
   - integrate with TUI message palette for one-keystroke sending

4. **Queue becomes first-class**
   - `forge send` should be queue-first
   - add `forge queue edit` for fast reorder/insert (no full TUI required)

5. **One-command bootstrap**
   - `forge up --agents 8 --recipe coding-flywheel`
   - prints *exactly* what it did and the next best command to run
