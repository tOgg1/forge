# Rust Post-Release Verification Checklist Dry-Run (2026-02-10)

Task: `forge-63f`  
Mode: dry-run against current repo + smoke probes

## Command run

```bash
test -f docs/rust-post-release-verification-checklist.md \
  && scripts/rust-loop-tui-smoke.sh \
  && scripts/rust-fmail-tui-smoke.sh
```

Result: pass

## Notes

- `cargo test -p fmail-tui --lib` currently fails locally due to untracked/in-progress `agents` view work in `rust/crates/fmail-tui/src/agents.rs` referencing adapter APIs that do not exist (`TextRole::Error`, `RenderFrame::to_text`). Not part of this checklist dry-run; needs owner fix before treating `fmail-tui` as green.

