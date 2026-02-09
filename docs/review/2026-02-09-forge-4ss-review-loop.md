# Review: `forge-4ss` (iteration result persistence)

## Findings

1. **High**: interrupt status parity miss (`killed` missing in Rust helper)
   - Rust `iteration_result` status enum only has `Running|Success|Error` and `status_from_error` only maps `None -> Success`, `Some -> Error` (`rust/crates/forge-loop/src/iteration_result.rs:2`, `rust/crates/forge-loop/src/iteration_result.rs:16`).
   - Go runner explicitly persists `killed` on interrupt (`internal/loop/runner.go:519`).
   - Risk: interrupted runs can be persisted with wrong status once this helper is wired into persistence path.
   - Fix hint: add `Killed` variant + interrupt-aware mapping API (or pass explicit final status), plus regression test for interrupt -> `killed`.

2. **Medium**: unknown DB status coerced to `running`
   - Rust scan path defaults parse failures to `Running` (`rust/crates/forge-db/src/loop_run_repository.rs:266`).
   - Go scan path does not coerce to running; it preserves raw DB status text (`internal/db/loop_run_repository.go:218`).
   - Risk: malformed/corrupt status rows get silently misreported as active `running`.
   - Fix hint: remove `unwrap_or_default` coercion; preserve unknown status or surface explicit validation error. Add malformed-status test.

## Validation

- `cargo test -p forge-loop` ✅ (127 tests incl scenario tests)
- `cargo test -p forge-db loop_run_repository` ✅
- `go test ./internal/loop ./internal/db` ❌ blocked by local Go toolchain mismatch:
  - compile errors show `go1.25.7` stdlib vs `go1.25.6` tool.

## Summary

- Result: **issues found** (2).
- Residual risk: Go parity validation for touched oracle packages is currently blocked until local Go version mismatch is resolved.
