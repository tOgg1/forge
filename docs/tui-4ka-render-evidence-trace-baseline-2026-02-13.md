# TUI-4ka render evidence + trace baseline capture

Date: 2026-02-13
Task: `forge-4ka`

## Commands run

```bash
scripts/rust-frankentui-bootstrap-smoke.sh
cargo test -p forge-tui --test layout_snapshot_test -- --nocapture
shasum -a 256 build/rust-frankentui-bootstrap-smoke.txt
shasum -a 256 crates/forge-tui/tests/golden/layout/*.txt
```

## Result

- Bootstrap smoke: PASS
- Capture artifact: `build/rust-frankentui-bootstrap-smoke.txt`
- Snapshot gate: PASS (`key_layout_snapshots_across_breakpoints`)
- Baseline hash manifest: `docs/tui-render-baseline-2026-02-13.sha256`

## Regression diff use

- Re-run the same commands, then compare hash manifest outputs.
- Any hash drift indicates render/trace baseline change requiring review.
