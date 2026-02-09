# Review: forge-5dc (CLI root/global flags/error envelope)

Date: 2026-02-09  
Reviewer: rust-full-review-1

## Findings

1. **High**: `--config` and `--chdir` parsed but never applied.
   - Evidence:
     - Parser stores values: `rust/crates/forge-cli/src/error_envelope.rs:61`
     - Root dispatcher never applies them before command execution: `rust/crates/forge-cli/src/lib.rs:57`
     - Probe: `cargo run -q -p forge-cli -- --config /definitely/not/real.yaml ps` => exit `0`, `No loops found`.
     - Probe: `cargo run -q -p forge-cli -- --chdir /definitely/not/a/dir ps` => exit `0`, `No loops found`.
   - Parity risk: Go root applies `chdir` and config load during init/preflight (`internal/cli/root.go:99`, `internal/cli/root.go:106`, `internal/cli/root.go:119`) and fails fast on invalid paths/config.
   - Fix hint: root pre-dispatch setup phase for `chdir` + config load; return classified error on failures.

2. **High**: `--robot-help` flag is dead (parsed only).
   - Evidence:
     - Flag parsed: `rust/crates/forge-cli/src/error_envelope.rs:59`
     - No handler branch in root dispatch: `rust/crates/forge-cli/src/lib.rs:67`
     - Probe: `cargo run -q -p forge-cli -- --robot-help` prints standard help, not robot-help schema.
   - Parity risk: Go emits dedicated robot-help payload and exits 0 (`internal/cli/root.go:72`, `internal/cli/robot_help_test.go:10`).
   - Fix hint: add early `robot_help` branch before normal command routing.

3. **Medium**: unknown root flag classified as unknown command.
   - Evidence:
     - Unknown command fallback: `rust/crates/forge-cli/src/lib.rs:188`
     - Probe: `cargo run -q -p forge-cli -- --definitely-not-a-real-flag` => `unknown forge command: --definitely-not-a-real-flag` + full help.
   - Parity risk: Go snapshot expects `unknown flag: --definitely-not-a-real-flag` (`docs/forge/help/forge-root-invalid-flag.stderr.txt:1`).
   - Fix hint: root-level detection for unrecognized `-`/`--` tokens, classify as invalid flag (`ERR_INVALID_FLAG` in JSON modes).

4. **Medium**: no-args behavior drift.
   - Evidence:
     - Rust: empty command branch renders help: `rust/crates/forge-cli/src/lib.rs:68`
     - Go: default root action runs TUI: `internal/cli/root.go:53`
   - Parity risk: root UX contract mismatch (help claims no-args launches TUI).
   - Fix hint: map no-args root path to TUI/preflight entrypoint.

5. **Medium**: regression test gaps for root parity.
   - Evidence:
     - Existing root tests focus on help/version/unknown-command only: `rust/crates/forge-cli/tests/root_command_test.rs:6`
     - Missing cases: `--robot-help`, invalid-root-flag shape, `--config` invalid path failure, `--chdir` invalid path failure, no-args TUI default behavior.
   - Fix hint: add root parity tests + golden fixtures for error surface.

## Validation Notes

- Ran: `cargo test -p forge-cli --test root_command_test` (pass).
- Ran: `cargo test -p forge-cli --lib` (pass).
- Attempted Go parity tests: `go test ./internal/cli -run 'TestRoot|TestRobotHelp|TestWatchRequiresJSONL|TestForgeRootSnapshotsCurrent'` (blocked by local Go toolchain mismatch: stdlib/object version `go1.25.7` vs tool `go1.25.6`).
