# Swarm UX Journeys and Pain Points

This document captures the core user journeys, friction points, and UX
opportunities for Swarm. It is grounded in the product spec and the current
codebase. It is intended to guide implementation sequencing and UX polish.

## Snapshot: Current Reality

- The CLI surface is partially implemented in code, but the shipped binary may
  not expose all commands unless rebuilt. `swarm --help` currently lists only
  `migrate` plus base helpers.
- The TUI is a placeholder; there is no visible dashboard workflow yet.
- Many commands are spec-defined but still marked "planned" in docs.

This gap is an immediate trust risk: users see a promised workflow but cannot
find it in the product. The UX plan below prioritizes reconciling this.

## Journey 1: First run and initialization

### Steps
1. Build Swarm.
2. Create config file.
3. Run database migrations.
4. Launch `swarm` for TUI or run CLI commands.

### Pain points
- No guided first-run flow; users must know the sequence.
- Missing dependencies (tmux, ssh) are surfaced late and without clear fixes.
- Errors do not suggest a single "next step".
- TUI is a stub and does not provide immediate value.

### Opportunities
- Introduce `swarm init` to create config, run migrations, and verify
  prerequisites.
- Add preflight checks to all high-impact commands.
- Show a successful "next steps" summary after init.

### Dependencies
- `swarm-eli6.2.1` `swarm init`
- `swarm-eli6.2.2` preflight errors

## Journey 2: Add or bootstrap a node

### Steps
1. `swarm node add --name <name> --ssh <target>` or `--local`.
2. `swarm node doctor <node>` to verify dependencies.
3. Optional `swarm node bootstrap --ssh root@host`.

### Pain points
- No progress feedback for long operations.
- Errors are terse; missing tmux or auth issues are not explained well.
- Node names and IDs are not interchangeable in a consistent way.

### Opportunities
- Add progress indicators for doctor/bootstrap.
- Standardize name/ID matching and error suggestions.
- Explicitly show local vs remote nodes in output.

### Dependencies
- `swarm-eli6.3.6` progress indicators
- `swarm-eli6.2.4` short-ID resolution

## Journey 3: Create or import a workspace

### Steps
1. `swarm ws create --path <repo> [--node <node>]`.
2. Optional: `swarm ws import --session <tmux> --node <node>`.
3. `swarm ws status <ws>` or `swarm ws attach`.

### Pain points
- Path validation errors lack clear hints.
- Session naming is opaque and not previewed.
- No clear warning before destructive actions (`ws remove --destroy`).

### Opportunities
- Preflight checks for path validity and git repo detection.
- Provide a dry-run summary (session name, node, repo path).
- Standardize confirmations for destructive actions.

### Dependencies
- `swarm-eli6.2.2` preflight
- `swarm-eli6.2.3` confirmations

## Journey 4: Spawn and manage agents

### Steps
1. `swarm agent spawn --workspace <ws> --type opencode`.
2. `swarm agent list`, `swarm agent status`.
3. `swarm agent send`, `pause`, `resume`, `interrupt`, `restart`.

### Pain points
- Message input is single-line and clumsy for long prompts.
- Output is inconsistent between list/status and other commands.
- Not obvious which agent is "ready" or "blocked".

### Opportunities
- Multi-line input for `agent send`.
- Unified status icons and a CLI style guide.
- Add a fleet-level summary for quick health checks.

### Dependencies
- `swarm-eli6.3.1` structured tables
- `swarm-eli6.3.2` icons/colors
- `swarm-eli6.3.4` multi-line send
- `swarm-eli6.3.5` fleet summary

## Journey 5: Monitor progress and state

### Steps
1. `swarm agent list` or `swarm agent status`.
2. `swarm export status` (planned) or TUI view (planned).

### Pain points
- No consolidated "overview" command.
- No streaming/watch mode for state changes.
- State confidence and reasons are not visually emphasized.

### Opportunities
- Add `swarm status` for fleet summary (reuse export status).
- Implement `--watch` JSONL for automated dashboards.
- Make state reason/confidence visible and consistent.

### Dependencies
- `swarm-eli6.3.5` summary
- `swarm-h4jd` watch mode

## Journey 6: Approvals and safety prompts

### Steps
1. Agent triggers approval-required state.
2. Operator approves/denies.

### Pain points
- No approval inbox yet.
- No clear indication on cards or CLI that an approval is pending.

### Opportunities
- Approvals inbox in TUI and CLI actions to approve/deny.
- Highlight blocking approvals in summaries.

### Dependencies
- Existing approvals tasks (`swarm-0bnp`, `swarm-3q4s`, `swarm-646g`).

## Journey 7: Error recovery and resilience

### Steps
1. Agent fails, pane missing, or SSH command fails.
2. Operator restarts or reconfigures.

### Pain points
- Errors often lack actionable guidance.
- No standardized "fix it" steps or recommended commands.

### Opportunities
- Structured error envelopes with hints.
- Add a troubleshooting guide with copy-paste fixes.

### Dependencies
- `swarm-eli6.3.3` JSON error envelope
- `swarm-eli6.5.2` troubleshooting guide

## Journey 8: Offboarding and cleanup

### Steps
1. Remove agents/workspaces/nodes.
2. Clear queues.

### Pain points
- Destructive actions are not consistently confirmed.
- No summary of what will be deleted.

### Opportunities
- Standardized confirmations and `--yes` for automation.
- Explicit impact summary (agents/panes/workspaces).

### Dependencies
- `swarm-eli6.2.3` confirmations

## Cross-cutting UX opportunities (priority order)

1. First-run wizard + preflight checks
2. CLI output consistency and status semantics
3. Fleet summary and watch mode
4. Destructive action safety
5. Message input ergonomics
6. TUI empty states and refresh cues

## Key blockers to resolve early

- CLI style guide (`swarm-eli6.1.2`)
- Terminology alignment (`swarm-eli6.1.4`)
- Non-interactive mode (`swarm-j39n`)
