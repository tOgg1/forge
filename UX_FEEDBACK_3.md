[Download the three “pointers” (docs + skill + OpenCode plugin skeleton)](sandbox:/mnt/data/forge-ux-pointers.zip)

Inside that zip you’ll find these **new files** added to your repo:

1. `docs/ux/forge-cli-v2.md`
2. `.claude/skills/forge-mail/SKILL.md`
3. `.opencode/plugin/forge-mail.ts`

These are written with the **Forge** name and with **OpenCode-first** as the default assumption.

---

## 1) Pointer: Forge CLI v2 command tree + help text

I put the full spec (tree + “copy/pasteable” Cobra help text) into:

* `docs/ux/forge-cli-v2.md`

### The core UX decisions in this revision

#### Queue-first by default

If you want *real* multi-agent control, “send” should mean **enqueue + scheduler dispatch** (safe, repeatable, automation-friendly).

* `forge send` → **enqueue**
* `forge inject` → **immediate tmux injection** (explicitly dangerous)

This directly supports your “queue up messages / wait for usage / continue later” workflow because the *queue becomes the truth*.

#### Add “templates” + “sequences” as first-class concepts

You asked for:

* message palettes
* stored message templates
* stored sequences (macros)
* rapid multi-agent spin-up

Those need a CLI surface that’s dead simple:

* `forge template add|edit|run`
* `forge seq add|edit|run`

…and they should map 1:1 into the TUI message palette.

#### Add a context system (`forge use`)

You should not have to pass `--workspace` / `--agent` constantly.

* `forge use <workspace|agent>` sets “current target”
* then `forge send "continue"` works immediately

This is the same “kubectl-like” ergonomics that makes CLIs feel *fast*.

#### Top-level “fast path” aliases

Your CLI can stay clean while still being power-user friendly:

* `forge` → TUI (default)
* `forge up` → create/open workspace + spawn default agents
* `forge ls` → alias `forge ws list`
* `forge ps` → alias `forge agent list`

### Why I didn’t recommend skills duplicated in both `.opencode/skill` and `.claude/skills`

OpenCode **already** discovers Claude-compatible skills from `.claude/skills/<name>/SKILL.md`. ([OpenCode][1])
It also warns that skill names should be unique across locations. ([OpenCode][1])
So the best UX/DX is: **keep Forge Mail skill in `.claude/skills/forge-mail/` only** and OpenCode will still pick it up. ([OpenCode][1])

---

## 2) Pointer: Forge Mail skill (Claude + OpenCode compatible)

I created:

* `.claude/skills/forge-mail/SKILL.md`

This follows the Claude skill structure (folder with `SKILL.md` + YAML frontmatter). ([Claude Code][2])
It also stays within OpenCode’s recognized frontmatter fields (OpenCode ignores unknown fields anyway). ([OpenCode][1])

### What this skill accomplishes

It teaches agents:

* when to use mail vs queue vs direct injection
* how to write actionable handoff messages (so no follow-up questions needed)
* how to use advisory file locking safely (TTL, conflict behavior, release early)
* consistent subject/body conventions so mailbox search works

### What to add next (QoL from MCP Agent Mail)

Right now the skill is a “best practices + how-to use the tools” doc. The next DX leap is to add supporting files (allowed by Claude skills) like templates and snippets next to `SKILL.md`. ([Claude Code][2])
Examples:

* `templates/handoff.md`
* `templates/review-request.md`
* `templates/conflict-resolution.md`

Then the **TUI message palette** can directly list these and insert them into the queue editor.

---

## 3) Pointer: OpenCode plugin skeleton exposing `forge_mail_*` and `forge_lock_*`

I created:

* `.opencode/plugin/forge-mail.ts`

OpenCode loads plugins from `.opencode/plugin` (project) or `~/.config/opencode/plugin` (global). ([OpenCode][3])
Plugins can define **custom tools** using `tool(...)`, and can execute commands using Bun’s `$` shell API. ([OpenCode][3])

### Tools included in the skeleton

* `forge_mail_send`
* `forge_mail_inbox`
* `forge_mail_read`
* `forge_mail_ack`
* `forge_lock_claim`
* `forge_lock_release`
* `forge_lock_status`

The skeleton assumes you will expose matching CLI commands like:

* `forge mail send|inbox|read|ack --json`
* `forge lock claim|release|status --json`

That’s important: **your agents can now “use Forge Mail outside the codebase”** because OpenCode tools become the agent’s interface (not MCP). This answers your earlier concern about “how can agents themselves utilize it?”

---

## CLI redesign critique: what’s missing today for “best UX”

Based on the repo’s current CLI surface, here are the key UX gaps and the concrete fixes:

### 1) Queue is present, but not “productized”

You already have queue concepts (message/pause/conditional) in code, but from a user POV the system needs:

* a **single mental model**: “send = enqueue, scheduler dispatches”
* visibility: “why isn’t this message sending?”
* one-shot commands to manipulate queue items fast

**CLI fixes**

* `forge queue ls --agent A1` shows status: pending / dispatched / blocked + reason (idle gate, cooldown, paused)
* `forge explain <agent|queue-item>` prints a human sentence:

  * “Blocked: account cooldown until 18:42:13”
  * “Blocked: agent is busy”
  * “Blocked: waiting for previous item completion”
* `forge wait --agent A1 --until idle|queue-empty|cooldown-over` (this is huge for automation)

### 2) No “recipes” for mass spawning + initial tasking

Spinning up 8–20 agents should feel like one action.

**CLI fix**

* `forge up --agents 8 --recipe baseline`
* recipes define:

  * agent types/models/accounts rotation strategy
  * an initial sequence to enqueue to each agent

This becomes the backbone for your “spin up a large amount of agents quickly” story.

### 3) No template/sequence subsystem

Without templates + sequences you’ll *always* feel like you’re manually copy/pasting.

**CLI fix**

* `forge template add …` opens `$EDITOR`
* `forge seq add …` opens `$EDITOR`
* `forge template run X` enqueues a message with variables
* `forge seq run Y` enqueues multiple items (including pauses/conditions)

### 4) Too many “almost overlapping” command surfaces

You have (or will have) `status`, `export status`, `watch`, `audit`, etc.

**UX rule**

* One “human default”: `forge status`
* One “dashboards default”: `forge watch --jsonl`
* Everything else is subcommands or flags

---

## TUI redesign notes: what to change to make it feel “elite”

Your TUI already has a strong foundation:

* command palette
* queue editor UI (even if demo-bound)
* mailbox view
* approvals toggles
* inspector panel

Here’s how I’d push it into “best-in-class” territory.

### A) Add a Message Palette separate from the Command Palette

You want:

* stored templates
* stored sequences
* rapid dispatch to one/many agents

**Design**

* `Ctrl+K` = command palette (navigation/actions)
* `Ctrl+P` = message palette (templates + sequences)
* selecting an entry:

  * prompts for target (selected agent / selected workspace / multi-select)
  * prompts for variables (if template has vars)
  * chooses enqueue mode:

    * end of queue (default)
    * front of queue
    * “enqueue + after cooldown”
    * “enqueue + when idle”

This turns template usage into a **two-keystroke operation**.

### B) Multi-select + bulk actions is mandatory for real swarms

You need:

* “send this template to all idle agents”
* “pause these 6 agents for 10m”
* “rotate accounts on selected agents”

**Design**

* Space = toggle selection on an agent card
* Shift+Space = select range
* Then:

  * `P` pause/resume selected
  * `T` open message palette targeting selected
  * `Q` open queue editor for “bulk enqueue” mode

### C) Queue editor needs “timeline” semantics

Right now queue editor is “list editing”.
For elite UX, it should show:

* item type icon: message / pause / conditional
* gating reason (if conditional)
* “blocked until time” (cooldown)
* dispatch attempts / errors
* quick reorder

**Hotkeys**

* `j/k` move cursor
* `J/K` move item down/up
* `i` insert message
* `p` insert pause
* `g` insert “when idle” gate
* `t` insert template (opens message palette)

### D) “Launchpad” wizard: spawn + task agents in one flow

A dedicated modal (think: `N` for “new swarm batch”):

1. Choose workspace (or create from current repo)
2. Choose agent type (OpenCode default)
3. Choose count
4. Choose account rotation mode (round-robin profiles)
5. Choose initial sequence (“bootstrap”, “bugfix loop”, “PR review”, etc.)
6. Confirm → it spawns agents + enqueues the sequence to each

This is the single highest-leverage UX feature for your goal.

---

## Quick note: why the OpenCode plugin is a big strategic win

You’re right that OpenCode being hackable is a competitive advantage.

Because:

* OpenCode lets you add custom tools via plugins. ([OpenCode][3])
* Those tools become the agent’s “native” interface to your system.
* That’s the cleanest path to a **non-MCP** mail/lock layer that agents can still use everywhere.

This is exactly why I made the plugin skeleton call out `forge_mail_*` and `forge_lock_*` tools.

---

## What I recommend you do next (in order)

1. **Rename CLI binary and root command to `forge`** (and update help output)
2. Implement **context** (`forge use`, plus implicit repo-based workspace selection)
3. Implement **templates + sequences** (file-backed first; DB later)
4. Wire up the **scheduler** so queue dispatch is real (this is crucial for your “pause/continue/cooldown” story)
5. TUI: add **Message Palette + Launchpad wizard + multi-select**

If you want, paste your current `internal/cli` command list (or tell me which commands you want to keep vs break) and I’ll produce a “migration map” (old → new) that minimizes churn while still hitting the UX you want.

[1]: https://opencode.ai/docs/skills/ "Agent Skills | OpenCode"
[2]: https://code.claude.com/docs/en/skills "Agent Skills - Claude Code Docs"
[3]: https://opencode.ai/docs/plugins/ "Plugins | OpenCode"

