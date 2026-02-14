# TUI-B4H degradation policy tuner

## Scope
- Add deterministic policy tuning for constrained terminals/links.
- Provide explicit quality knobs for adaptive runtime behavior.

## Changes
- Added `crates/forge-tui/src/degradation_policy.rs`:
  - `DegradationPolicyMode` (`Off`, `Balanced`, `Aggressive`)
  - `DegradationSignals` runtime input model
  - `DegradationDecision` output model
  - `tune_degradation_policy(...)` scoring + decision logic
  - reason-code emission for operator/debug visibility
- Exported module from `crates/forge-tui/src/lib.rs`.

## Behavior
- `Off`: no adaptive degradation.
- `Balanced`: degrade when frame/input/transport or viewport constraints cross moderate thresholds.
- `Aggressive`: earlier degradation thresholds and stronger feature caps.
- Tuned outputs include density/motion toggles, syntax-highlighting gate, log line cap, poll interval, and max multi-layout cap.

## Validation
- `cargo test -p forge-tui degradation_policy::tests:: -- --nocapture`
- `cargo build -p forge-tui`
