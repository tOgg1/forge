# TUI-602 claim feed and ownership conflicts

Status: implemented in `crates/forge-tui/src/app.rs`.

Scope delivered:
- Inbox now includes claim timeline data (`ClaimEventView`) with recency ordering.
- Conflict detection flags tasks claimed by different agents over time.
- Conflict alerts shown inline in timeline (`!` marker) and conflict count in Inbox header.
- Safe resolution workflow shortcuts in Inbox:
  - `o`: focus next claim conflict and show who conflicts with who
  - `O`: show explicit takeover-resolution command hint

Resolution guidance surfaced in TUI status:
- `fmail send task "takeover claim: <task-id> by <agent>"`

Notes:
- Claim feed is app-state driven (`set_claim_events`), so host can hydrate from fmail task logs.
