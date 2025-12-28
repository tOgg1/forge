Below is a **cohesive, “rigid” improvement plan** that’s grounded in what your repo already declares itself to be: **a control plane for running/supervising AI coding agents across repos and servers, via SSH + tmux, with optional per-node daemon (`forged`)**, backed by a **SQLite + event log state engine**.

I’m also incorporating the key constraints and direction from your transcript:

* **Forge is the worker layer** (you don’t want “coordinator agents” inside this repo).
* You want **much tighter CASS integration** (session search + cass-memory style learning).
* You’re considering replacing **MCP Agent Mail** with a **non‑MCP, cross‑server messaging + file-locking layer** that agents can actually use.

One note: I can see your top-level structure, README, and bootstrap installer (and infer a lot from the presence of `proto/` + `gen/`), but I can’t reliably read *all* internal source files through GitHub’s HTML views in this environment. So I’m going to be very explicit about **what I’m confident you already have** vs **what I’m recommending you implement**.

---

## 1) What you already have (as “source of truth” today)

From your README and repo layout, the system is already shaped like this:

* **Control plane**: TUI + CLI + scheduler sitting over a **State Engine (SQLite + event log)**.
* **Nodes**: local/remote machines controlled via **SSH and tmux**, plus an optional per-node daemon `forged` for “Mode B” better performance/real-time ops.
* **Core concepts** are already defined and stable: `Node`, `Workspace`, `Agent`, `Queue`.
* You ship a **bootstrap installer** that downloads `bootstrap.sh` + checksum and runs it as root (with verification).
* You’ve already committed to an RPC boundary: you have `proto/forged/v1` and `gen/forged/v1`, which implies you’re on track for a clean daemon API boundary.

That’s a good base. The next improvements should be about **hardening, standardizing interfaces, and making integration (agents + CASS + mail/locks) “real” rather than heuristic**.

---

## 2) The real root problem you’re about to hit

Everything you want—queues, pauses, cooldowns, multi-account switching, “working/idle/waiting permission” states, and cross-server messaging—depends on one thing:

### You need a *reliable* way to observe and control agent processes.

If “agent control” is mostly `tmux send-keys` + “guess state from terminal text”, you’ll get:

* flaky state detection,
* broken automation after upstream CLI updates,
* hard-to-reproduce bugs (“it was idle but we thought it was working”),
* brittle cooldown enforcement (no durable mapping between “account session” and “agent process”).

So the main architectural push should be:

> **Make agent processes first-class managed entities** with a standard runtime harness, and treat tmux as a *presentation + input transport*, not your core abstraction.

OpenCode is a big advantage here precisely because it’s hackable, but you still want a fallback that works for other CLIs without forking them.

---

## 3) Rigid plan: how to improve the codebase in a way that scales

I’m going to structure this as phases with crisp deliverables. The rule is: **finish each phase with testable invariants** and a CLI surface you can depend on from your future “coordinator layer”.

### Phase 0 — Establish non-negotiable invariants (foundation for everything)

**Deliverables**

1. **Define a canonical Agent State Machine** (in code + docs), used everywhere:

   * `STARTING`
   * `IDLE`
   * `BUSY`
   * `WAITING_PERMISSION`
   * `COOLDOWN`
   * `ERROR`
   * `STOPPED`
2. **Define canonical events** emitted into the event log for every transition:
   `agent.started`, `agent.idle`, `agent.busy`, `agent.waiting_permission`, `agent.cooldown_started`, `agent.cooldown_ended`, `agent.crashed`, `agent.stopped`, `queue.enqueued`, `queue.dispatched`, `queue.blocked`, etc.
3. Add **state invariants** enforced in code (panic or fail fast in tests):

   * An agent can only be in **one** of the above states at a time.
   * A queue item can only be **pending → dispatched → acked/failed/canceled**.
   * Every dispatched message has an audit trail: **who/what dispatched it**.

**Why this matters**
Without this, you’ll keep patching “minor issues” forever because every bug becomes “state drift”.

---

### Phase 1 — Introduce an “Agent Runner” (this is the biggest leverage point)

**Core idea**
Instead of running `claude` / `codex` / `opencode` directly in the tmux pane, run:

```
tmux pane command: forge-agent-runner --workspace W --agent A -- <actual agent cli...>
```

**What the runner does**

* Owns a PTY around the agent CLI process (even inside tmux).
* Emits structured events:

  * last input sent
  * last output lines (tail)
  * heartbeat timestamps
  * detected “prompt ready” vs “thinking”
* Implements a **small, explicit control protocol**:

  * `SendMessage(text)` (with optional “press enter” semantics)
  * `Pause(duration)` / `Cooldown(until)`
  * `SwapAccount(account_id)` (if supported)
* Writes events into your existing event log / sqlite.

**Deliverables**

* `internal/agent/runner` package + a `cmd/forge-agent-runner` binary.
* A minimal integration test that spins up:

  * a fake “agent CLI” (a script that prints prompts + simulates “busy/idle”)
  * tmux session + pane
  * verifies dispatch → busy → idle transition deterministically.

**Why this matters**
Once you have a runner, all your “minor issues” get easier because:

* tmux is no longer the place you infer state from,
* you standardize control/telemetry across very different CLIs.

---

### Phase 2 — Make tmux/ssh “transport layers” with idempotent operations

Right now you likely have a bunch of tmux/ssh operations sprinkled around.

**Deliverables**

1. A strict interface boundary:

   * `TmuxTransport`: create session, list panes, send keys, capture pane, etc.
   * `SSHTransport`: run command, copy file, check remote binary versions, etc.
2. Every tmux operation becomes **idempotent** and safe to retry:

   * “create session” checks existence
   * “create pane” checks target exists
   * “inject message” includes verification step if possible (or at least logs the exact send-keys)
3. Add `forge doctor`:

   * verifies tmux installed, versions ok
   * verifies `forged` reachable if in daemon mode
   * verifies DB migrations applied

**Outcome**
You stop debugging “it worked yesterday but not today” issues caused by transport drift.

---

### Phase 3 — Replace heuristic scheduling with a deterministic queue engine

You already describe message queuing + conditional dispatch + pauses/cooldowns.
Now make it deterministic.

**Deliverables**

* A queue engine that runs off:

  * current `AgentState`
  * account cooldown state
  * queue item constraints (`requires_idle`, `requires_permission=false`, etc.)
* A single “scheduler tick” function that’s easy to unit test:

  * input: snapshot of state
  * output: list of actions to take (dispatch msg, start cooldown, etc.)
* Tests:

  * “dispatch only when idle”
  * “insert cooldown after N messages”
  * “don’t dispatch if WAITING_PERMISSION unless message is permission-grant”

This pays off later when your “coordinator layer” starts interacting via CLI.

---

## 4) Rolling your own mail + file locking layer (and how agents can use it)

You’re asking the right question: **“how can agents use it outside the forge codebase?”**

The answer is: **give agents a stable tool surface**. Agents don’t need to import Go packages; they need something they can call.

### The right shape: “Forge Mail v2” as a local tool + API

**Design principle**

* **Agents interact via a CLI tool** available in PATH on the node.
* The CLI tool talks to local `forged` (unix socket) or remote `forged` (grpc) depending on workspace config.
* The storage is your SQLite (or per-workspace sqlite) so it’s durable and queryable.

### Minimal primitives you need (don’t overbuild)

#### Messaging

* `forge mail send --to <agent|workspace|broadcast> --body <...> [--thread <id>]`
* `forge mail inbox --agent <id> --json`
* `forge mail ack --agent <id> --msg <id>`

#### File reservations (locks)

* `forge lock acquire --workspace W --agent A --path path --ttl 20m --reason "..."`
* `forge lock release --workspace W --agent A --path path`
* `forge lock ls --workspace W --json`

**Critical behavior**

* Locks are **leases** (TTL + renew) so you never deadlock permanently.
* Conflicts return structured data (who holds, when it expires, reason).

### How agents “utilise it”

* In OpenCode: you can implement it as a tool/plugin (best UX).
* In Claude Code / Codex / Gemini CLIs: you can instruct the agent to run shell commands:

  * `forge mail inbox --json`
  * `forge lock acquire ...`

Even if a CLI is closed, almost all of them allow shell execution in some form. Your orchestration layer can also call this tool on behalf of an agent when needed.

### Why this beats MCP Agent Mail for your goals

* Not bound to MCP transport assumptions.
* Can be cross-server via your own daemon/API.
* You can optimize for your specific workflow (locks + queues + multi-node).

---

## 5) OpenCode as a competitive advantage: how to lean into it properly

Your instinct is correct: **OpenCode is your best “first-class” integration target** because you can make it *observable* and *controllable* in a principled way.

### The key advantage isn’t “arbitrary models”

It’s this:

> You can add a structured event stream and tool hooks so Forge doesn’t have to guess.

Concrete recommendation:

* Build an OpenCode integration that emits events like:

  * “prompt ready”
  * “model call started/ended”
  * “token usage”
  * “needs user confirmation”
  * “plan updated”
* Then your runner can stop parsing terminal text and just consume OpenCode events.

### What to do with other CLIs

Support them, but put them into a **tiered capability model**:

* **Tier 0 (Basic):** send-keys + “prompt regex” idle detection.
* **Tier 1 (Better):** runner-level PTY + output parsing + explicit heartbeat.
* **Tier 2 (Best):** native hooks/plugin/events (OpenCode likely lives here).

This keeps the codebase sane: you don’t redesign the whole system around the least hackable CLI.

---

## 6) CASS + cass-memory: do it “for real”, not as a bolt-on

You want strict integration here. That means:

### Two directions of flow

#### A) Forge → CASS (write/search index)

* Every agent session should produce:

  * a durable transcript/log file location
  * metadata: repo, workspace, node, agent type, start/end, outcome
* Provide a command:
  `forge cass index --workspace W --agent A --session <id>`

#### B) CASS → Forge (context injection)

* Before dispatching a new task (or even before queueing), your coordinator layer should be able to ask:

  * `forge cass search --query "..."`
  * `forge cass context --task "..."`
* Forge can expose these as CLI calls returning JSON, so the upper layer can decide what to feed into agents.

### cass-memory style learning loop (minimal viable)

Don’t build a giant “learning system” yet. Do this first:

* Record outcomes per task/session:

  * success/failure
  * duration
  * retries
  * interruptions
* Attach them to a “pattern id” or “workflow id” that your coordinator layer assigns.
* Use that later for ranking strategies.

---

## 7) “Minor issues” triage: the brutal, high-ROI fixes you should do next

Even without seeing your internal code, these are the recurring pain points in systems like this—and the fixes are mechanical:

1. **Make every operation auditable**

   * Every injected keystroke/message gets an ID, timestamp, agent, workspace, and is persisted.
   * Every “pause inserted” has a reason (cooldown, account rotation, manual, policy).

2. **Kill hidden concurrency**

   * One scheduler loop owns queue dispatch per workspace or per agent.
   * All writes happen through one state engine API (transactional).

3. **Crash recovery**

   * If tmux pane disappears, runner emits `agent.crashed`, queue items go back to pending (or “blocked”) with clear reason.

4. **Workspace bootstrap from existing tmux session**

   * Must be deterministic:

     * inspect tmux session name, panes, current working directory per pane if possible
     * if ambiguous, store “workspace requires folder selection” state (don’t half-create).

5. **Golden path UX**

   * TUI should answer in 2 seconds:

     * What is working?
     * What is stuck?
     * What needs my permission?
     * What is cooling down?

---

## 8) Immediate “next iteration” backlog (ordered, concrete)

If you want a tight iteration cycle, do exactly this sequence:

1. **Implement Agent Runner (Phase 1)**

   * Even a minimal runner that only does: heartbeat + send message + detect prompt-ready.

2. **Wire runner events into your event log**

   * Add `agent.state_changed` events.
   * Update TUI to show states from the event log, not from polling.

3. **Deterministic queue tick + tests**

   * One pure function for scheduling decisions.
   * Unit tests for cooldown + permission gating.

4. **Mail/Lock v2 MVP**

   * Local sqlite tables + CLI commands + gRPC endpoints (if daemon mode).
   * One end-to-end test: agent acquires lock → other agent gets conflict.

5. **CASS hooks**

   * Start with:

     * `forge cass search --json`
     * `forge cass index session`
   * Don’t overcomplicate; just make it reliable and scriptable.

---

## 9) One naming/packaging suggestion (because it affects UX + CLI)

You noted earlier “Forge” is generic. I agree. If you rename, do it *before* the CLI becomes widely used, because command names fossilize.

My practical naming heuristic:

* short CLI (`forge`, `guild`, `lattice`, `workbench`)
* daemon suffix (`forged`, `guildd`, etc.)
* repo name matches binary

But I’d postpone a rename until **after** the runner + queue engine stabilize, unless you’re already about to publish.

