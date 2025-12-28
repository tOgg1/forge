Got it. I dug through the zip and I’m going to be blunt: you have **a lot of solid substrate already** (DB + models + adapters + ssh/tmux primitives + a daemon + a state engine + a scheduler + a TUI skeleton), but it’s currently **not wired into a single, obvious “happy path”**. That’s why the CLI feels unusable: it exposes many low-level levers, but the system doesn’t yet “just work” end‑to‑end.

Below is (1) my read of what you have today, (2) the **highest-impact fixes**, and (3) a **rigid plan** to turn this into **Forge** (OpenCode-first) with Codex support, a much friendlier CLI, and Forge Mail + locking delivered as **Claude/OpenCode skills**.

---

## 1) What you have right now (as I understand the code)

### Control plane (CLI)

* `cmd/forge/main.go` runs a Cobra CLI.
* Command families exist and are fairly extensive:

  * `init`, `migrate`, `node`, `ws` (workspace), `agent`, `queue`, `watch`, `status`, `export`, `vault`, `accounts`, plus output formatting (`--json`, `--jsonl`, etc).
* UX intent is documented in `docs/ux/cli-style.md`, but the **commands still feel “plumbing-first”** rather than “workflow-first.”

### Persistence + domain model (good foundation)

* SQLite schema + migrations under `internal/db/`.
* Models under `internal/models/` are surprisingly complete: nodes, workspaces, agents, queue items, events, usage, approvals, alerts.
* You already have **room for richer integration**: `AgentAdapter.DetectState(screen, meta any)` supports passing metadata, but your state engine currently passes `nil` (more on that below).

### Execution layer (tmux + adapters)

* `internal/tmux/` is a proper command wrapper for tmux.
* `internal/adapters/` defines adapters for OpenCode, Claude Code, Codex, Gemini, plus a generic fallback adapter (screen scraping).
* `internal/agent/service.go` spawns agents into panes and injects messages via tmux send-keys.

### State + queue machinery exists (but not fully connected)

* `internal/state/` includes an Engine and Poller to periodically inspect agents and emit state events.
* `internal/scheduler/` exists and looks like it’s meant to dispatch queue items when agents are idle and accounts aren’t cooling down.
* But: these pieces **aren’t currently tied into a “run loop”** that the user can start and trust.

### Node + daemon layer exists (but underused)

* `cmd/forged/main.go` + `internal/forged/` implements a per-node gRPC daemon that can spawn agents, stream panes, etc.
* `internal/node/client.go` is actually quite good: it supports **daemon mode** (SSH-tunneled gRPC) and **SSH fallback mode**.

**Key problem:** your *workspace* and *agent* services currently use a **local tmux client**, so they don’t leverage the node/daemon abstraction you’ve already built. Remote is “designed,” but not truly “end-to-end.”

### TUI exists (but currently not “live”)

* `internal/tui/` is built on Charm libs (Bubble Tea + Lip Gloss), and the UI scaffolding is there.
* But `internal/cli/ui.go` launches the TUI with `StateEngine: nil`, so it renders mostly sample/static data.

---

## 2) Critical issues to fix first (these are blocking)

### A) Your Go toolchain version is wrong

Your `go.mod` declares `go 1.25.5`, which triggers toolchain download and breaks offline builds. That’s going to bite every new machine/CI environment.

**Fix:**

* Set `go` to a real, available version you’re targeting (ex: `1.23.x`).
* If you want toolchain pinning, use `toolchain go1.23.x` *in addition* (not a future version).

This alone will make onboarding and CI drastically less painful.

### B) Remote execution architecture exists, but your “core services” don’t use it

You have `internal/node/client.go` and `internal/forged`, but:

* `workspace.Service` creates tmux sessions using a `func() *tmux.Client` factory that can’t vary per node.
* `agent.Service` is instantiated with `tmux.NewLocalClient()` in CLI commands.

Net effect: **you can register nodes, but the actual agent/workspace lifecycle is still “local-first.”**

### C) Your state engine is not using adapter metadata

`AgentAdapter.DetectState(screen, meta any)` supports richer detection, but `internal/state/engine.go` calls `DetectState(screen, nil)`.

That’s a straight bug/unfinished wiring and it prevents “native” integrations from ever becoming real.

### D) UX problem: no “happy path”

Even if the system technically works, it currently doesn’t provide a single obvious workflow like:

> “I’m in a repo → create/open workspace → spawn OpenCode agent → send instructions → watch status”

This is the main reason you “don’t understand how to use it yourself.”

---

## 3) Decision: OpenCode-only first is absolutely the right move

OpenCode gives you:

* A built-in **server control surface** (OpenAPI endpoints and SSE event stream) so you don’t have to screen-scrape tmux to know what’s happening.
* A **plugin system** to add tools and subscribe to events, which is perfect for Forge Mail + locks as “native tools.”
* A **skills system** compatible with Claude skills (and OpenCode can load Claude-compatible skills too).

This is your competitive advantage: you can build *reliable orchestration* on a *structured API*, instead of constantly fighting terminal UI heuristics.

---

## 4) Rename plan: finalize Forge naming (do it immediately, but safely)

### 4.1 Naming conventions

* CLI binary: `forge`
* Node daemon: `forged` (keeps the “d” convention)
* Config dir: `~/.config/forge/`
* Data dir: `~/.local/share/forge/`
* Env vars: `FORGE_*` (but keep `SWARM_*` as deprecated aliases for one release to avoid breaking yourself mid-iteration)

### 4.2 Mechanical changes checklist

* Rename commands/binaries:

  * Ensure CLI binary is `forge`
  * Ensure daemon binary is `forged`
  * Update `Makefile`, `scripts/*`, and service unit names accordingly
* Update module path:

  * Change `go.mod` module name to your real repo module path.
* Update Cobra root:

  * `Use: "forge"` and adjust help text.
* Update config defaults:

  * `internal/config/config.go` constants and paths.
* Update docs/spec:

  * `PRODUCT_SPECIFICATION.md` heading + terminology.

### 4.3 Backward-compat migration behavior

On startup:

* If `~/.config/forge/` missing but `~/.config/forge/` exists → auto-copy and print:

  * “Migrated Forge config to Forge. Set FORGE_* env vars going forward.”

This avoids a “rename broke everything” week.

---

## 5) Rigid implementation plan (phased, but each phase is shippable)

### Phase 1 — Make Forge usable locally with OpenCode-only (workflow-first CLI)

**Goal:** one machine, one repo, one workspace, one OpenCode agent, send messages reliably, see status.

#### 1) Create a single “happy path” command: `forge up`

`forge up` should:

1. Ensure DB exists + migrations applied automatically (remove “migrate up” as a user step).
2. Detect current git repo root (or accept `--path`).
3. Create (or reuse) a workspace for that path on the local node.
4. Spawn an OpenCode agent (default name: `oc-1`) in that workspace.
5. Print “next steps” that are **actionable**.

**UX target output example:**

* `Workspace: forge://local/my-repo`
* `Agent: oc-1 (id …)`
* Next:

  * `forge send oc-1 "…"`,
  * `forge attach oc-1`,
  * `forge watch --workspace my-repo`

#### 2) Add ergonomic top-level shortcuts

Right now the command tree exists but it’s not “fast.” Add these aliases:

* `forge ls` → summary of workspaces + agents
* `forge ps` → list agents with state
* `forge send <agent|workspace> "<msg>"`
* `forge attach <agent|workspace>` (tmux attach / select-pane)
* `forge log <agent>` (tail transcript)
* `forge doctor` (dependency checks: tmux, opencode, ssh, permissions)

Keep the detailed command families (`forge ws …`, `forge agent …`) but put the “90% use case” up top.

#### 3) Fix state engine metadata bug immediately

In `internal/state/engine.go`:

* pass `agent.Metadata` into `DetectState(screen, meta)` (instead of `nil`)

Even before you go “OpenCode-native,” this is an obvious correctness fix.

---

### Phase 2 — OpenCode-native control (stop relying on tmux injection)

**Goal:** message injection + state tracking should work even if the tmux pane isn’t focused, and should not depend on fragile UI prompts.

OpenCode gives you the building blocks:

* It exposes server endpoints (OpenAPI `/doc`) and event streams via SSE (`/event` and `/global/event`).
* It exposes TUI endpoints to manipulate prompt state (`/tui/append-prompt`, `/tui/submit-prompt`) and session endpoints (`/session`, `/session/{id}/message`, etc.).

#### 1) Store OpenCode connection info in `Agent.Metadata`

Add fields (example):

* `opencode.host`
* `opencode.port`
* `opencode.session_id`
* `opencode.base_url` (derived)

#### 2) Spawn model: “one OpenCode server per agent”

OpenCode TUI can be started with a known host/port, and can connect to an existing server.

Your spawn flow should become:

1. Allocate a port per agent (node-local).
2. Start `opencode serve --port X --hostname 127.0.0.1` (or equivalent)
3. In tmux pane, run `opencode --port X --hostname 127.0.0.1`
4. Save host/port into metadata.

#### 3) Replace `tmux send-keys` with OpenCode prompt API

Implement:

* `forge send agent "msg"` → `POST /tui/append-prompt` then `POST /tui/submit-prompt` on the agent’s server.

This will be your biggest reliability win.

#### 4) Replace “screen scraping state detection” with SSE events

Run an OpenCode event watcher (per agent or multiplexed):

* Subscribe to `/event` or `/global/event`
* Update agent state in DB when you receive:

  * idle/busy signals
  * permission prompts
  * errors / tool results

Even OpenCode’s plugin event list shows the kinds of events you can rely on (session idle, permission updated, file edited, etc.).

---

### Phase 3 — Codex support (without API keys) via OpenCode

You specifically called out `opencode-openai-codex-auth`. That’s the right approach if you want “Codex subscription style” auth rather than raw OpenAI API keys.

From that repo’s README:

* It’s an OpenCode plugin that enables OpenCode to use the Codex backend using **ChatGPT Plus/Pro OAuth authentication**, so you consume your subscription instead of API credits.
* Install is done by adding the plugin to OpenCode config and pinning the version (it doesn’t auto-update).
* You authenticate with `opencode auth login` and choose a provider option for ChatGPT Plus/Pro Codex subscription.

#### Deliverables

1. `forge opencode plugin install codex-auth`

   * Writes/patches OpenCode config to include the plugin + pinned version.
2. `forge account add --provider openai-codex --profile …`

   * Integrates with your existing vault/profile machinery (copy auth blobs).
3. `forge agent spawn --type opencode --account <profile>`

   * Ensures the correct profile is active before spawn.

#### Important operational detail

That plugin README warns about a port conflict with the official Codex CLI (1455). Bake this into `forge doctor` so users see it before it hurts.

---

### Phase 4 — Forge Mail + file locking (MCP-free), usable by agents “outside” the codebase

You asked: **how can agents use this “outside” the Forge codebase?**

Answer: make Forge Mail a **local service + CLI** that runs on every node, and expose it through:

1. a CLI (`forge mail …`, `forge lock …`) that any agent can call in a shell, and
2. an OpenCode plugin that offers “tools” so OpenCode can call it natively.

OpenCode plugins are designed exactly for this: you can define tools and subscribe to events.

#### 1) What to copy from MCP Agent Mail (QoL feature set)

Agent Mail’s concept doc describes a system with:

* Agent identities + profiles
* Mail stored in SQLite and optionally materialized into git (human-readable artifacts)
* File reservations/leases and a “who owns what” view

You don’t need MCP to replicate the *product experience*. You need:

* a message store,
* a lock table,
* optional “materialization to files” for transparency,
* and a stable CLI/tools interface.

#### 2) Forge Mail MVP spec

**DB tables (central or per-node; start per-node):**

* `mail_threads`
* `mail_messages`
* `file_locks`

**CLI surface:**

* `forge mail send --to <agent|workspace> --subject … --body …`
* `forge mail inbox [--agent …]`
* `forge mail ack <msg-id>`
* `forge lock claim <path/glob> [--exclusive] [--ttl …]`
* `forge lock release <path/glob>`
* `forge lock ls`

**Behavior:**

* Locks have TTL (expire automatically).
* Conflicts are explicit, not silent.
* Every lock/message emits an event (so your dashboard can surface it).

#### 3) OpenCode-native tool integration

Ship an OpenCode plugin under `.opencode/plugin/forge-mail/opencode.ts` that exposes tools like:

* `forge_mail_inbox`
* `forge_mail_send`
* `forge_lock_claim`
* `forge_lock_release`
* `forge_lock_status`

This is aligned with OpenCode’s plugin model (tools + event subscription).

---

### Phase 5 — Skills: make Forge Mail discoverable to agents (Claude + OpenCode)

This is the cleanest part because you can write one skill and have both ecosystems pick it up.

**Claude Code skills** live in:

* Project: `.claude/skills/<name>/SKILL.md`
* User: `~/.claude/skills/<name>/SKILL.md`

**OpenCode skills** live in:

* Project: `.opencode/skill/<name>/SKILL.md`
* User: `~/.opencode/skill/<name>/SKILL.md`
* And it can also load Claude-compatible `.claude/skills`.

#### What you should do

* Add: `.claude/skills/forge-mail/SKILL.md`
* Optionally mirror it: `.opencode/skill/forge-mail/SKILL.md` (or just rely on OpenCode’s Claude-compatible loading).

**Skill contents (what it must teach):**

* How to claim/release files before editing
* How to check inbox and ack messages
* How to route messages to “the orchestrator” vs other workers
* Conventions: subjects, thread IDs, when to escalate

This directly addresses your “agents must be able to use it” requirement.

---

## 6) CLI redesign (concrete structure)

Here’s the CLI structure I’d ship for Forge v0.1:

### Primary workflow commands (the ones you use daily)

* `forge up`
* `forge ls` (workspaces)
* `forge ps` (agents)
* `forge send <agent|workspace> "<msg>"`
* `forge attach <agent|workspace>`
* `forge watch [--workspace …] [--agent …]`
* `forge doctor`
* `forge mail …`
* `forge lock …`

### Power / admin

* `forge node …`
* `forge ws …`
* `forge agent …`
* `forge account …`
* `forge vault …`
* `forge export status --json`

### Rules for UX

* Every command prints **a next step** when it succeeds.
* Every “ID required” accepts:

  * full ID, prefix, name, and (for workspaces) repo path
* Default output is human-friendly; `--json` and `--jsonl` are stable and well-documented.

This will make Forge feel like a product instead of a toolkit.

---

## 7) What I would *not* do yet (to keep you moving)

* Do **not** try to perfect multi-provider CLI support right now.
* Do **not** over-invest in tmux screen scraping once OpenCode-native is in place.
* Do **not** build “coordinator agents” into Forge (you already decided that belongs above the worker layer).

---

## 8) The shortest path to “this feels real” (my recommended order)

1. **Rename to Forge + fix Go version**
2. Implement `forge up`, `forge send`, `forge ps`, `forge attach`
3. Fix adapter metadata plumbing (state engine passing `meta`)
4. Implement OpenCode-native message injection via `/tui/append-prompt` + `/tui/submit-prompt`
5. Implement OpenCode-native state updates via SSE `/event`
6. Add Codex subscription auth by wiring `opencode-openai-codex-auth` install + profile switching
7. Add Forge Mail + locks (CLI first), then OpenCode plugin tools
8. Add `.claude/skills/forge-mail/SKILL.md` (works for Claude Code, and OpenCode can load it too)
9. Only then: wire the TUI to live state and make it “sexy.”
