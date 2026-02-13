# TUI-919 triage score queue

Task: `forge-6ad` (supporting model)  
Status: delivered

## Scope

- Rank next operator actions with deterministic scoring.
- Combine urgency, risk, staleness, blocked status, and ownership bias.

## Implementation

- New module: `crates/forge-tui/src/triage_score_queue.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core API:

- `rank_triage_queue(samples, context)`

Core model:

- `TriageQueueSample`
- `TriageQueueWeights`
- `TriageQueueContext`
- `TriageQueueItem`
- `TriageQueueReport`

## Validation

- `cargo test -p forge-tui --lib triage_score_queue::tests::`
- `cargo build -p forge-tui`
