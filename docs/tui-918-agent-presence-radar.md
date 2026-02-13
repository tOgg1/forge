# TUI-918 agent presence radar

Task: `forge-y10`  
Status: delivered (module + tests; workspace validation currently blocked by unrelated `forge-cli` compile errors)

## Scope

- Live presence indicators for active operators.
- Classify agents into `active`, `idle`, `stuck`, `offline`, `unknown`.
- Produce sortable rows and summary counts for panel rendering.

## Implementation

- New module: `crates/forge-tui/src/agent_presence_radar.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core API:

- `build_agent_presence_radar(samples, now_epoch_s, policy)`

Core model:

- `AgentPresenceSample`
- `AgentPresencePolicy`
- `AgentPresenceState`
- `PresenceSeverity`
- `AgentPresenceRow`
- `AgentPresenceSummary`
- `AgentPresenceRadar`

Behavior:

- Merges duplicate samples per agent.
- Uses latest heartbeat/progress timestamps.
- Computes idle ages with future-timestamp clamping.
- Detects `stuck` only when in-progress work exists.
- Sorts rows by severity then idle age for fast triage.

## Regression tests

Added in `crates/forge-tui/src/agent_presence_radar.rs`:

- mixed-state classification and summary counts
- duplicate sample merge behavior
- future timestamp clamping
- no-task stale-progress fallback (`idle` vs `stuck`)

## Validation

Attempted:

- `cargo test -p forge-tui agent_presence_radar -- --nocapture`
- `cargo build -p forge-tui`

Blocked by unrelated existing `forge-cli` errors in
`crates/forge-cli/src/workflow_run_persistence.rs` (`E0252` duplicate imports).
