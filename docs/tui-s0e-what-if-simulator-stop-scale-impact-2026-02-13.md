# TUI-S0E what-if simulator (stop/scale impact)

## Scope
- Add deterministic forecast primitives to preview likely control-action impact.

## Changes
- Added `crates/forge-tui/src/what_if_simulator.rs`:
  - operational input models (`FleetStateSnapshot`, `LoopOperationalState`)
  - action model (`Stop`, `Restart`, `ScaleFleet`)
  - forecast output (`ActionImpactForecast`, queue/failure risk, confidence, notes)
  - simulator function: `simulate_action_impact(...)`
- Exported module via `crates/forge-tui/src/lib.rs`.

## Validation
- `cargo test -p forge-tui what_if_simulator::tests:: -- --nocapture`
- `cargo build -p forge-tui`
