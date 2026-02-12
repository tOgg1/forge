# TUI-606 communication quality checks and stale-thread alerts

Task: `forge-z33`  
Status: delivered

## Scope

- Detect unanswered asks in collaboration threads.
- Detect stale coordination threads with active tasks.
- Detect missing closure notes on terminal tasks.
- Provide concrete corrective action suggestions.

## Implementation

- New module: `crates/forge-tui/src/communication_quality.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core model:

- `CommunicationThreadSample`: normalized per-thread signal input
- `CommunicationQualityPolicy`: threshold policy for unanswered/stale checks
- `CommunicationAlert`: typed alert with severity/reasons/suggestion
- `CommunicationQualityReport`: sorted alerts + summary counters

Detection rules:

- `UnansweredAsk`: open asks older than policy threshold
- `StaleThread`: no thread activity beyond stale threshold while task status is active
- `MissingClosureNote`: terminal task status without closure note

Corrective actions:

- per-alert `command_hint` for `fmail send task ...`
- per-alert checklist for safe follow-up communication

## Regression tests

Added tests in `crates/forge-tui/src/communication_quality.rs` for:

- unanswered-ask escalation detection
- stale-thread detection on active tasks
- missing-closure-note detection on terminal tasks
- closure-note suppression when note exists
- deterministic alert sort order by severity/idle age

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
