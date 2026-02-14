# TUI-925 multi-node compare split view

Task: `forge-txz`  
Status: delivered

## Scope

- Compare node state/log drift side-by-side against a baseline node.
- Rank peers by divergence severity for fast operator triage.
- Provide panel-ready split lines for compact TUI rendering.

## Implementation

- Added module: `crates/forge-tui/src/multi_node_compare_split.rs`
- Added compare model:
  - `NodeCompareSample`
  - `MultiNodeCompareConfig`
  - `NodeCompareDelta`
  - `MultiNodeCompareReport`
- Added compare engine:
  - `build_multi_node_compare_report(samples, config)`
  - Baseline selection:
    - preferred `baseline_node_id`
    - fallback to first valid node if missing
  - Divergence scoring components:
    - status mismatch penalty
    - queue/error deltas
    - cpu/memory deltas
    - last-log mismatch bonus
  - Sorted output by highest divergence first.
- Added split renderer:
  - `render_multi_node_compare_split(report, width, height)`
- Exported module in `crates/forge-tui/src/lib.rs`.

## Validation

- `cargo test -p forge-tui multi_node_compare_split::tests::`
- `cargo build -p forge-tui`
