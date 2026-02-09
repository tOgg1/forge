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

---

## Iteration: `forge-qe5` (repository parity pass)

### Findings

1. **High**: non-atomic port allocation path can race under contention
   - Rust `allocate` performs `find_available_port` and `INSERT` outside a transaction (`rust/crates/forge-db/src/port_repository.rs:134`).
   - Go path does both in one DB transaction (`internal/db/port_repository.go:70`).
   - Risk: duplicate-port race/failed allocation bursts under concurrent allocators.
   - Fix hint: run select+insert in same transaction (or retry wrapper) to preserve parity semantics.

2. **High**: `set_current` unique-conflict fallback swallows write failures
   - On unique conflict, fallback `tx.execute(...)` result is discarded and function returns `Ok(())` (`rust/crates/forge-db/src/loop_work_state_repository.rs:204`).
   - Risk: false-success write path; caller sees success even when fallback update fails.
   - Fix hint: check fallback update result and return `DbError::Transaction` on failure.

3. **Medium**: unknown loop-run status coerced to `running`
   - Scanner uses `parse(...).unwrap_or_default()` (`rust/crates/forge-db/src/loop_run_repository.rs:266`), defaulting unknown status to `Running`.
   - Go scanner preserves raw status text (`internal/db/loop_run_repository.go:218`).
   - Risk: malformed rows can be misreported as active/running.
   - Fix hint: preserve unknown status explicitly or return validation error; add malformed-status regression test.

### Validation

- `cargo test -p forge-db` run reached `file_lock_repository_test` failure unrelated to reviewed files; earlier reviewed repository tests in that run passed before failure.
- Targeted rerun blocked by concurrent workspace churn introducing `event_repository` compile error:
  - `rust/crates/forge-db/src/event_repository.rs:246` (`Option<String>::flatten()`).
- `go test ./internal/db/...` blocked by local toolchain mismatch:
  - stdlib compiled with `go1.25.7`, tool is `go1.25.6`.

### Summary

- Result: **issues found** (3).

---

## Iteration: `forge-qe5` (mail repository delta)

### Findings

1. No concrete defects found in reviewed delta (`rust/crates/forge-db/src/mail_repository.rs`, `rust/crates/forge-db/tests/mail_repository_test.rs`, `rust/crates/forge-db/src/lib.rs`).

### Validation

- `cargo test -p forge-db --test mail_repository_test` ✅ (3 passed)
- `cargo test -p forge-db --test migration_006_test` ✅ (1 passed)
- `go test ./internal/db/...` ❌ blocked by local Go toolchain mismatch:
  - stdlib compiled with `go1.25.7`, tool is `go1.25.6`.

### Summary

- Result: **pass**.
- Residual risk: Go parity checks remain blocked by toolchain mismatch; broadcast inbox cross-workspace isolation is not explicitly covered by current Rust tests.
