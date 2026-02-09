# Review: forge-mjb (Loop runtime ledger writer behavior)

## Findings

1. **Medium (fixed): ledger file mode parity drift under shared-group umask (`0002`)**
   - Location: `rust/crates/forge-loop/src/ledger_writer.rs:81`, `rust/crates/forge-loop/src/ledger_writer.rs:122`
   - Issue: Rust used default create mode (`0666 & ~umask`), while Go sets `0644 & ~umask`. Under `umask 0002`, Rust produced group-writable ledgers (`0664`) unlike Go (`0644`).
   - Fix applied: set Unix create mode explicitly via `OpenOptionsExt::mode(0o644)` for both initial create and append-create paths.

## Validation

- `cargo test -p forge-loop ledger_writer --manifest-path rust/Cargo.toml` passed.
- `go test ./internal/loop/...` blocked by local toolchain mismatch (`stdlib go1.25.7` vs `tool go1.25.6`).

## Residual Risk

- Go parity suite not runnable in this environment until Go toolchain versions are aligned.
