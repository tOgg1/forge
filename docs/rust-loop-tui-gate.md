# Rust Loop TUI Parity Gate

Task: `forge-fgt`  
Status: in-progress

## Goal

Define strict acceptance criteria for loop TUI parity before Rust cutover.

## Scope

- Go source workflows:
  - `internal/looptui`
  - `internal/tui/components`
- Rust target crate:
  - `forge-tui`

## Required gate criteria

1. Workflow parity
- Required workflows:
  - loop list/status navigation
  - loop detail/log panes
  - queue visibility and queue action feedback
  - stop/kill/resume controls
- Every workflow needs one reproducible evidence item (smoke output or manual checklist item).

2. Failure-state parity
- Required failure-state coverage:
  - selected loop disappears while navigating
  - running loop delete prompt requires force
  - error-state render path remains readable and non-crashing
- Failure-state checks need scripted probe coverage + manual checklist verification.

3. Keymap parity
- Critical keybindings must remain equivalent for operator muscle memory.
- Any keybinding divergence requires explicit parity exception note and owner sign-off.

4. Performance/readability parity
- Log rendering remains responsive for active loops.
- No regressions in status readability (state, error reason, queue depth).
- Smoke baseline: no UI panic/crash under loop refresh churn.

5. Cutover rule
- Loop TUI cutover is blocked until all loop TUI gate items are marked green with evidence.

## Evidence + checks

- Manual checklist artifact:
  - `docs/rust-loop-tui-checklist.md`
  - `docs/rust-release-gate-checklist.md` (loop TUI section).
- Baseline smoke command:
  - `scripts/rust-loop-tui-smoke.sh`
  - `env -u GOROOT -u GOTOOLDIR go test ./internal/looptui -count=1`
  - targeted Rust `forge-tui` probes for workflow + failure states.
- Matrix link:
  - `docs/rust-parity-matrix.md` loop TUI row references this gate doc.
