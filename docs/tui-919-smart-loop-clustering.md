# tui-919: Smart loop clustering

## Scope
- Task: `forge-xne`
- Goal: auto-group loops by inferred work domain and surface groups in Overview.

## Implementation
- Added domain clustering module: `crates/forge-tui/src/smart_loop_clustering.rs`
  - `cluster_loops_by_domain(...)`
  - `compact_domain_summary(...)`
- Inference sources (priority):
  - repo crate path (`.../crates/<name>/...`)
  - repo leaf directory
  - loop name token
  - profile name
  - pool name
- Confidence scoring tracks inference quality per group.

## Overview rendering
- `crates/forge-tui/src/overview_tab.rs`
  - Added `Work Domains (Auto)` panel when layout has enough vertical space.
  - Panel shows:
    - `top: <domain> (count) ...`
    - `groups:<N> loops:<M>`
  - Keeps existing workflow hint at bottom.

## Tests
- New clustering tests in `smart_loop_clustering.rs`:
  - crate-path grouping
  - fallback to name/profile
  - compact summary rendering
- New overview regression test:
  - `paneled_overview_shows_work_domains_when_space_allows`
