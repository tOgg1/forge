# TUI-608 multi-log cockpit sticky headers + health strip

Task: `forge-333`

What shipped:
- Upgraded mini-pane health strip to show compact per-loop signal:
- `status`, `queue depth`, `runs`, `health flag`, `harness`.
- Added regression for sticky cockpit rows:
- Header + health + separator stay fixed while log body advances on new tail data.
- Strengthened truncation safety for Unicode-heavy log lines.

Files:
- `crates/forge-tui/src/multi_logs.rs`
- `crates/forge-tui/tests/golden/layout/multi_logs_80x24.txt`
- `crates/forge-tui/tests/golden/layout/multi_logs_120x40.txt`
- `crates/forge-tui/tests/golden/layout/multi_logs_200x50.txt`

Validation:
- `cargo fmt --check`
- `cargo clippy -p forge-tui --all-targets -- -D warnings`
- `cargo test -p forge-tui`
