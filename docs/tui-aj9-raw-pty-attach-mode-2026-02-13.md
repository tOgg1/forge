# forge-aj9: Raw PTY attach mode

Date: 2026-02-13
Task: `forge-aj9`

## Summary

Added raw PTY attach core primitives for TUI power mode:

- attach-plan builder for tmux pane and local socket transports
- attach/detach session state
- monotonic stream chunk ingestion with bounded byte buffer
- ANSI/control-sequence sanitizing overlay renderer for raw stream tail

Goal covered: attach to raw agent terminal stream and show it in a compact TUI overlay model.

## Files

- `crates/forge-tui/src/raw_pty_attach.rs`
- `crates/forge-tui/src/lib.rs`

## Validation

```bash
cargo test -p forge-tui raw_pty_attach::tests:: -- --nocapture
cargo build -p forge-tui
```
