# TUI-501 swarm template library and spawn presets

Task: `forge-73b`
Status: delivered

## What shipped

- New `forge-tui` template library module: `crates/forge-tui/src/swarm_templates.rs`.
- Reusable topology templates: `small`, `medium`, `full`.
- Per-template profile maps and spawn presets (lane/profile/prompt/count).
- Per-template guardrail defaults:
  - stale takeover threshold (`45` minutes)
  - claim-broadcast required (`true`)
  - full validation before close required (`true`)
  - max parallel claims (`1`)
- Lookup helper: `find_swarm_template` (id/title matching; case-insensitive).
- Exported module from crate root (`crates/forge-tui/src/lib.rs`).

## Validation

- Unit coverage in `crates/forge-tui/src/swarm_templates.rs` for:
  - library order
  - profile map + preset + guardrail invariants
  - lookup behavior
  - full template capacity + auditor lane presence
